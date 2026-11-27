mod api;
mod connection;

use crate::voice::echo_path::EchoPath;
use crate::voice::machine::Fault;
use crate::voice::streams::{MicLink, Opened, SpeakerLink};
use crate::voice::underrun::{self, Reading};
use api::{BufferAttr, SampleSpec, Stream};
use connection::{Connection, Wake};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::Arc;

const PLAYBACK_BLOCKS: u32 = 4;
const FRAME_BYTES: usize = size_of::<f32>();

pub(super) fn available() -> bool {
    api::api().is_ok()
}

struct Shared {
    wake: *const Wake,
    underrun: bool,
    underflowed: bool,
}

impl Shared {
    unsafe fn signal(&self) {
        // SAFETY: `wake` belongs to the connection the link's `Arc` keeps alive; the caller holds the lock.
        unsafe { ((*self.wake).api.signal)((*self.wake).mainloop, 0) };
    }

    unsafe fn refresh(&self, stream: *mut Stream) {
        // SAFETY: the caller holds the lock and `stream` is live; the operation is released at once.
        unsafe {
            let api = (*self.wake).api;
            let operation = (api.update_timing)(stream, std::ptr::null(), std::ptr::null_mut());
            if !operation.is_null() {
                (api.operation_unref)(operation);
            }
        }
    }
}

unsafe extern "C" fn wake_stream(_stream: *mut Stream, data: *mut c_void) {
    // SAFETY: `data` is the link's `Shared`, which outlives the stream's callbacks; callbacks run with the lock held.
    unsafe { (*data.cast::<Shared>()).signal() };
}

unsafe extern "C" fn wake_request(stream: *mut Stream, _bytes: usize, data: *mut c_void) {
    // SAFETY: as for `wake_stream`.
    unsafe { wake_stream(stream, data) };
}

unsafe extern "C" fn underflow(stream: *mut Stream, data: *mut c_void) {
    // SAFETY: `data` is the link's `Shared`; callbacks run with the lock held, so nothing else touches it.
    let shared = unsafe { &mut *data.cast::<Shared>() };
    shared.underrun = true;
    shared.underflowed = true;
    // SAFETY: the lock is held and `stream` is live.
    unsafe {
        shared.refresh(stream);
        shared.signal();
    }
}

unsafe extern "C" fn started(stream: *mut Stream, data: *mut c_void) {
    // SAFETY: as for `underflow`.
    let shared = unsafe { &mut *data.cast::<Shared>() };
    shared.underrun = false;
    // SAFETY: as for `underflow`.
    unsafe {
        shared.refresh(stream);
        shared.signal();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Playback,
    Record,
}

struct Link {
    connection: Arc<Connection>,
    stream: *mut Stream,
    shared: Box<Shared>,
    what: &'static str,
}

// SAFETY: the stream is used by one thread at a time, and every libpulse call on it holds the mainloop's lock.
unsafe impl Send for Link {}

impl Link {
    fn open(connection: &Arc<Connection>, direction: Direction, rate: u32, block: usize) -> Result<Self, String> {
        let api = connection.api();
        let bytes = u32::try_from(block * FRAME_BYTES).unwrap_or(u32::MAX);
        let (name, what) = match direction {
            Direction::Playback => (c"Voice speaker", "speaker"),
            Direction::Record => (c"Voice microphone", "microphone"),
        };
        let spec = SampleSpec { format: api::SAMPLE_FLOAT32LE, rate, channels: 1 };
        let locked = connection.lock();
        // SAFETY: the lock is held and the context is ready; `spec` outlives the call.
        let stream = unsafe { (api.stream_new)(connection.context(), name.as_ptr(), &spec, std::ptr::null()) };
        if stream.is_null() {
            return Err(connection.error());
        }
        let shared = Box::new(Shared { wake: connection.wake(), underrun: false, underflowed: false });
        let mut link = Self { connection: connection.clone(), stream, shared, what };
        let data: *mut c_void = (&mut *link.shared as *mut Shared).cast();
        // SAFETY: the lock is held; `data` and the buffer attributes outlive the calls, and `Drop` clears these callbacks before `shared` is freed.
        let rc = unsafe {
            (api.on_state)(stream, Some(wake_stream), data);
            match direction {
                Direction::Playback => {
                    (api.on_write)(stream, Some(wake_request), data);
                    (api.on_underflow)(stream, Some(underflow), data);
                    (api.on_started)(stream, Some(started), data);
                    let attr = BufferAttr { maxlength: u32::MAX, tlength: bytes * PLAYBACK_BLOCKS, prebuf: bytes, minreq: bytes, fragsize: u32::MAX };
                    (api.connect_playback)(stream, std::ptr::null(), &attr, api::STREAM_TIMING, std::ptr::null(), std::ptr::null_mut())
                }
                Direction::Record => {
                    (api.on_read)(stream, Some(wake_request), data);
                    let attr = BufferAttr { maxlength: u32::MAX, tlength: u32::MAX, prebuf: u32::MAX, minreq: u32::MAX, fragsize: bytes };
                    (api.connect_record)(stream, std::ptr::null(), &attr, api::STREAM_TIMING)
                }
            }
        };
        if rc < 0 {
            return Err(connection.error());
        }
        loop {
            // SAFETY: the lock is held.
            match unsafe { (api.stream_state)(stream) } {
                api::STREAM_READY => break,
                api::STREAM_FAILED | api::STREAM_TERMINATED => return Err(connection.error()),
                _ => locked.wait(),
            }
        }
        drop(locked);
        Ok(link)
    }

    fn check(&self) -> Result<(), Fault> {
        // SAFETY: called with the lock held.
        let state = unsafe { (self.connection.api().stream_state)(self.stream) };
        if self.connection.failed() || state != api::STREAM_READY { Err(self.fault()) } else { Ok(()) }
    }

    fn fault(&self) -> Fault {
        Fault::passing(format!("PulseAudio {}: {}", self.what, self.connection.error()))
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        let api = self.connection.api();
        let _locked = self.connection.lock();
        // SAFETY: the lock is held; every callback that points at `shared` is cleared before the stream is released once.
        unsafe {
            (api.on_state)(self.stream, None, std::ptr::null_mut());
            (api.on_write)(self.stream, None, std::ptr::null_mut());
            (api.on_read)(self.stream, None, std::ptr::null_mut());
            (api.on_underflow)(self.stream, None, std::ptr::null_mut());
            (api.on_started)(self.stream, None, std::ptr::null_mut());
            (api.stream_disconnect)(self.stream);
            (api.stream_unref)(self.stream);
        }
    }
}

struct Heard {
    connection: Arc<Connection>,
    stream: *mut Stream,
}

// SAFETY: a reference to the record stream, only read with the mainloop's lock held.
unsafe impl Send for Heard {}

impl Drop for Heard {
    fn drop(&mut self) {
        let _locked = self.connection.lock();
        // SAFETY: releases the reference `open` took, with the lock held.
        unsafe { (self.connection.api().stream_unref)(self.stream) };
    }
}

pub(super) fn open(rate: u32, block: usize) -> Result<Opened, String> {
    let connection = Connection::open(api::api()?)?;
    let speaker = Link::open(&connection, Direction::Playback, rate, block)?;
    let mic = Link::open(&connection, Direction::Record, rate, block)?;
    let heard = {
        let _locked = connection.lock();
        // SAFETY: the lock is held and the record stream is live; `Heard` releases this reference.
        Heard { connection: connection.clone(), stream: unsafe { (connection.api().stream_ref)(mic.stream) } }
    };
    let pending = VecDeque::with_capacity(rate as usize);
    Ok(Opened {
        mic: Box::new(Mic { link: mic, pending, lost: 0 }),
        speaker: Box::new(Speaker { link: speaker, heard, rate, underrun: underrun::State::Waiting }),
        echo: EchoPath::InProcess,
    })
}

pub(super) struct Mic {
    link: Link,
    pending: VecDeque<f32>,
    lost: u64,
}

pub(super) struct Speaker {
    link: Link,
    heard: Heard,
    rate: u32,
    underrun: underrun::State,
}

impl Mic {
    fn push(&mut self, samples: impl ExactSizeIterator<Item = f32>) {
        let room = self.pending.capacity() - self.pending.len();
        let overflow = samples.len().saturating_sub(room);
        self.pending.drain(..overflow.min(self.pending.len()));
        self.pending.extend(samples);
    }
}

impl MicLink for Mic {
    fn read(&mut self, out: &mut [f32]) -> Result<(), Fault> {
        let api = self.link.connection.api();
        let stream = self.link.stream;
        let connection = self.link.connection.clone();
        let locked = connection.lock();
        while self.pending.len() < out.len() {
            self.link.check()?;
            // SAFETY: the lock is held and the stream is ready.
            let readable = unsafe { (api.readable)(stream) };
            if readable == usize::MAX {
                return Err(self.link.fault());
            }
            if readable == 0 {
                locked.wait();
                continue;
            }
            let (mut data, mut bytes): (*const c_void, usize) = (std::ptr::null(), 0);
            // SAFETY: the lock is held and the stream has data to read.
            if unsafe { (api.peek)(stream, &mut data, &mut bytes) } < 0 {
                return Err(self.link.fault());
            }
            if bytes == 0 {
                locked.wait();
                continue;
            }
            let frames = bytes / FRAME_BYTES;
            if data.is_null() {
                self.lost += frames as u64;
                self.push(std::iter::repeat_n(0.0, frames));
            } else {
                // SAFETY: `peek` returned `bytes` readable bytes of mono floats, valid until `drop`.
                let samples = unsafe { std::slice::from_raw_parts(data.cast::<f32>(), frames) };
                self.push(samples.iter().copied());
            }
            // SAFETY: releases the fragment `peek` returned, with the lock held.
            if unsafe { (api.drop)(stream) } < 0 {
                return Err(self.link.fault());
            }
        }
        drop(locked);
        let len = out.len();
        for (slot, sample) in out.iter_mut().zip(self.pending.drain(..len)) {
            *slot = sample;
        }
        Ok(())
    }

    fn take_lost(&mut self) -> u64 {
        std::mem::take(&mut self.lost)
    }
}

impl SpeakerLink for Speaker {
    fn write(&mut self, frames: &[f32]) -> Result<(), Fault> {
        let link = &self.link;
        let api = link.connection.api();
        let bytes = size_of_val(frames);
        let locked = link.connection.lock();
        loop {
            link.check()?;
            // SAFETY: the lock is held and the stream is ready.
            let writable = unsafe { (api.writable)(link.stream) };
            if writable == usize::MAX {
                return Err(link.fault());
            }
            if writable >= bytes {
                // SAFETY: the lock is held; libpulse copies `bytes` bytes from `frames` because no free callback is given.
                let rc = unsafe { (api.write)(link.stream, frames.as_ptr().cast(), bytes, std::ptr::null(), 0, api::SEEK_RELATIVE) };
                return if rc < 0 { Err(link.fault()) } else { Ok(()) };
            }
            locked.wait();
        }
    }

    fn take_silence(&mut self) -> u64 {
        let api = self.link.connection.api();
        let _locked = self.link.connection.lock();
        let (mut heard, mut played) = (0u64, 0u64);
        // SAFETY: the lock is held and both streams are live.
        let timed = unsafe { (api.stream_time)(self.heard.stream, &mut heard) >= 0 && (api.stream_time)(self.link.stream, &mut played) >= 0 };
        let shared = &mut *self.link.shared;
        let low = shared.underrun || std::mem::take(&mut shared.underflowed);
        // SAFETY: the lock is held and both streams are live.
        unsafe {
            shared.refresh(self.heard.stream);
            shared.refresh(self.link.stream);
        }
        if !timed {
            return 0;
        }
        let micros = i128::from(heard) - i128::from(played);
        let lead = i64::try_from(micros * i128::from(self.rate) / 1_000_000).unwrap_or(0);
        underrun::step(&mut self.underrun, Reading { lead, low })
    }
}

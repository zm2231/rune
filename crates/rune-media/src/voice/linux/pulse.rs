use crate::voice::echo_path::EchoPath;
use crate::voice::streams::{MicLink, Opened, SpeakerLink};
use crate::audio::linux::{dlopen_library, dlsym_fn, dlsym_opt};
use crate::voice::machine::Fault;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::OnceLock;

const PA_STREAM_PLAYBACK: c_int = 1;
const PA_STREAM_RECORD: c_int = 2;
const PA_SAMPLE_FLOAT32LE: c_int = 5;
const PLAYBACK_BLOCKS: u32 = 4;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

type New = unsafe extern "C" fn(
    server: *const c_char,
    name: *const c_char,
    dir: c_int,
    dev: *const c_char,
    stream_name: *const c_char,
    ss: *const SampleSpec,
    map: *const c_void,
    attr: *const BufferAttr,
    error: *mut c_int,
) -> *mut c_void;
type Read = unsafe extern "C" fn(s: *mut c_void, data: *mut c_void, bytes: usize, error: *mut c_int) -> c_int;
type Write = unsafe extern "C" fn(s: *mut c_void, data: *const c_void, bytes: usize, error: *mut c_int) -> c_int;
type Free = unsafe extern "C" fn(s: *mut c_void);
type Strerror = unsafe extern "C" fn(error: c_int) -> *const c_char;

#[derive(Clone, Copy)]
struct Api {
    new: New,
    read: Read,
    write: Write,
    free: Free,
    strerror: Option<Strerror>,
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

fn api() -> Result<Api, String> {
    API.get_or_init(|| {
        let handle = dlopen_library(c"libpulse-simple.so.0")?;
        // SAFETY: `handle` is libpulse-simple, and each type matches the symbol's C signature in `pulse/simple.h` and `pulse/error.h`.
        unsafe {
            Ok(Api {
                new: dlsym_fn(handle, c"pa_simple_new")?,
                read: dlsym_fn(handle, c"pa_simple_read")?,
                write: dlsym_fn(handle, c"pa_simple_write")?,
                free: dlsym_fn(handle, c"pa_simple_free")?,
                strerror: dlsym_opt(handle, c"pa_strerror"),
            })
        }
    })
    .clone()
}

pub(super) fn available() -> bool {
    api().is_ok()
}

fn error(api: Api, code: c_int) -> String {
    // SAFETY: `pa_strerror` returns a static NUL-terminated string or null for any code.
    let text = api.strerror.map(|f| unsafe { f(code) }).filter(|p| !p.is_null());
    // SAFETY: `text` is non-null and NUL-terminated.
    text.map(|p| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()).unwrap_or_else(|| format!("PulseAudio error {code}"))
}

struct Stream {
    api: Api,
    handle: *mut c_void,
}

impl Stream {
    fn open(api: Api, dir: c_int, rate: u32, block: usize) -> Result<Self, String> {
        let bytes = u32::try_from(block * 4).unwrap_or(u32::MAX);
        let attr = if dir == PA_STREAM_RECORD {
            BufferAttr { maxlength: u32::MAX, tlength: u32::MAX, prebuf: u32::MAX, minreq: u32::MAX, fragsize: bytes }
        } else {
            BufferAttr { maxlength: u32::MAX, tlength: bytes * PLAYBACK_BLOCKS, prebuf: bytes, minreq: bytes, fragsize: u32::MAX }
        };
        let spec = SampleSpec { format: PA_SAMPLE_FLOAT32LE, rate, channels: 1 };
        let name = if dir == PA_STREAM_RECORD { c"Voice microphone" } else { c"Voice speaker" };
        let mut code: c_int = 0;
        // SAFETY: every pointer is null or valid for the duration of the call.
        let handle = unsafe {
            (api.new)(std::ptr::null(), c"Rune".as_ptr(), dir, std::ptr::null(), name.as_ptr(), &spec, std::ptr::null(), &attr, &mut code)
        };
        if handle.is_null() { Err(error(api, code)) } else { Ok(Self { api, handle }) }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: `handle` came from `pa_simple_new` and is freed once.
        unsafe { (self.api.free)(self.handle) };
    }
}

// SAFETY: a stream's connection is owned exclusively and used from one thread at a time.
unsafe impl Send for Stream {}

pub(super) fn open(rate: u32, block: usize) -> Result<Opened, String> {
    let api = api()?;
    let speaker = Stream::open(api, PA_STREAM_PLAYBACK, rate, block)?;
    let mic = Stream::open(api, PA_STREAM_RECORD, rate, block)?;
    Ok(Opened { mic: Box::new(Mic(mic)), speaker: Box::new(Speaker(speaker)), echo: EchoPath::InProcess })
}

pub(super) struct Mic(Stream);

pub(super) struct Speaker(Stream);

impl MicLink for Mic {
    fn read(&mut self, out: &mut [f32]) -> Result<(), Fault> {
        let s = &self.0;
        let mut code: c_int = 0;
        // SAFETY: the stream is live and `out` holds `out.len()` writable floats.
        let rc = unsafe { (s.api.read)(s.handle, out.as_mut_ptr().cast(), size_of_val(out), &mut code) };
        if rc < 0 { Err(Fault::passing(format!("PulseAudio microphone: {}", error(s.api, code)))) } else { Ok(()) }
    }
}

impl SpeakerLink for Speaker {
    fn write(&mut self, frames: &[f32]) -> Result<(), Fault> {
        let s = &self.0;
        let mut code: c_int = 0;
        // SAFETY: the stream is live and `frames` holds `frames.len()` readable floats.
        let rc = unsafe { (s.api.write)(s.handle, frames.as_ptr().cast(), size_of_val(frames), &mut code) };
        if rc < 0 { Err(Fault::passing(format!("PulseAudio speaker: {}", error(s.api, code)))) } else { Ok(()) }
    }
}

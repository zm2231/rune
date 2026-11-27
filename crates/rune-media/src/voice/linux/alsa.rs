use crate::voice::echo_path::EchoPath;
use crate::voice::streams::{MicLink, Opened, SpeakerLink};
use crate::audio::linux::{dlopen_library, dlsym_fn, dlsym_opt};
use crate::voice::machine::Fault;
use std::ffi::{CStr, c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::sync::OnceLock;

const PLAYBACK: c_int = 0;
const CAPTURE: c_int = 1;
const FLOAT_LE: c_int = 14;
const RW_INTERLEAVED: c_int = 3;
const LATENCY_US: c_uint = 40_000;

type Open = unsafe extern "C" fn(pcm: *mut *mut c_void, name: *const c_char, stream: c_int, mode: c_int) -> c_int;
type Close = unsafe extern "C" fn(pcm: *mut c_void) -> c_int;
type SetParams = unsafe extern "C" fn(pcm: *mut c_void, format: c_int, access: c_int, channels: c_uint, rate: c_uint, soft_resample: c_int, latency: c_uint) -> c_int;
type Readi = unsafe extern "C" fn(pcm: *mut c_void, buffer: *mut c_void, size: c_ulong) -> c_long;
type Writei = unsafe extern "C" fn(pcm: *mut c_void, buffer: *const c_void, size: c_ulong) -> c_long;
type Recover = unsafe extern "C" fn(pcm: *mut c_void, err: c_int, silent: c_int) -> c_int;
type Strerror = unsafe extern "C" fn(errnum: c_int) -> *const c_char;

#[derive(Clone, Copy)]
struct Api {
    open: Open,
    close: Close,
    set_params: SetParams,
    readi: Readi,
    writei: Writei,
    recover: Recover,
    strerror: Option<Strerror>,
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

fn api() -> Result<Api, String> {
    API.get_or_init(|| {
        let handle = dlopen_library(c"libasound.so.2")?;
        // SAFETY: `handle` is libasound, and each type matches the symbol's C signature in `alsa/pcm.h` and `alsa/error.h`.
        unsafe {
            Ok(Api {
                open: dlsym_fn(handle, c"snd_pcm_open")?,
                close: dlsym_fn(handle, c"snd_pcm_close")?,
                set_params: dlsym_fn(handle, c"snd_pcm_set_params")?,
                readi: dlsym_fn(handle, c"snd_pcm_readi")?,
                writei: dlsym_fn(handle, c"snd_pcm_writei")?,
                recover: dlsym_fn(handle, c"snd_pcm_recover")?,
                strerror: dlsym_opt(handle, c"snd_strerror"),
            })
        }
    })
    .clone()
}

pub(super) fn available() -> bool {
    api().is_ok()
}

fn error(api: Api, code: c_int) -> String {
    // SAFETY: `snd_strerror` returns a static NUL-terminated string or null for any code.
    let text = api.strerror.map(|f| unsafe { f(code) }).filter(|p| !p.is_null());
    // SAFETY: `text` is non-null and NUL-terminated.
    text.map(|p| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()).unwrap_or_else(|| format!("ALSA error {code}"))
}

struct Pcm {
    api: Api,
    handle: *mut c_void,
}

impl Pcm {
    fn open(api: Api, stream: c_int, rate: u32) -> Result<Self, String> {
        let mut handle: *mut c_void = std::ptr::null_mut();
        // SAFETY: `handle` is a valid out-pointer and the name is NUL-terminated.
        let rc = unsafe { (api.open)(&mut handle, c"default".as_ptr(), stream, 0) };
        if rc < 0 || handle.is_null() {
            return Err(error(api, rc));
        }
        let pcm = Self { api, handle };
        // SAFETY: `pcm.handle` was just opened.
        let rc = unsafe { (api.set_params)(pcm.handle, FLOAT_LE, RW_INTERLEAVED, 1, rate, 1, LATENCY_US) };
        if rc < 0 { Err(error(api, rc)) } else { Ok(pcm) }
    }

    fn settle(&self, rc: c_long, what: &str) -> Result<usize, Fault> {
        if rc >= 0 {
            return Ok(usize::try_from(rc).unwrap_or(0));
        }
        let code = c_int::try_from(rc).unwrap_or(c_int::MIN);
        // SAFETY: the handle is live; `silent` keeps ALSA off stderr.
        if unsafe { (self.api.recover)(self.handle, code, 1) } == 0 {
            Ok(0)
        } else {
            Err(Fault::passing(format!("ALSA {what}: {}", error(self.api, code))))
        }
    }
}

impl Drop for Pcm {
    fn drop(&mut self) {
        // SAFETY: the handle came from `snd_pcm_open` and is closed once.
        unsafe { (self.api.close)(self.handle) };
    }
}

// SAFETY: a PCM handle is owned exclusively and used from one thread at a time.
unsafe impl Send for Pcm {}

pub(super) fn open(rate: u32) -> Result<Opened, String> {
    let api = api()?;
    let speaker = Pcm::open(api, PLAYBACK, rate)?;
    let mic = Pcm::open(api, CAPTURE, rate)?;
    Ok(Opened { mic: Box::new(Mic(mic)), speaker: Box::new(Speaker(speaker)), echo: EchoPath::InProcess })
}

pub(super) struct Mic(Pcm);

pub(super) struct Speaker(Pcm);

impl MicLink for Mic {
    fn read(&mut self, out: &mut [f32]) -> Result<(), Fault> {
        let pcm = &self.0;
        let mut at = 0;
        while at < out.len() {
            let rest = &mut out[at..];
            // SAFETY: the handle is live and `rest` holds `rest.len()` writable mono frames.
            let rc = unsafe { (pcm.api.readi)(pcm.handle, rest.as_mut_ptr().cast(), rest.len() as c_ulong) };
            at += pcm.settle(rc, "microphone")?;
        }
        Ok(())
    }
}

impl SpeakerLink for Speaker {
    fn write(&mut self, frames: &[f32]) -> Result<(), Fault> {
        let pcm = &self.0;
        let mut at = 0;
        while at < frames.len() {
            let rest = &frames[at..];
            // SAFETY: the handle is live and `rest` holds `rest.len()` readable mono frames.
            let rc = unsafe { (pcm.api.writei)(pcm.handle, rest.as_ptr().cast(), rest.len() as c_ulong) };
            at += pcm.settle(rc, "speaker")?;
        }
        Ok(())
    }
}

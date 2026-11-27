use crate::audio::linux::{dlopen_library, dlsym_fn};
use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::sync::OnceLock;

pub(super) const CONTEXT_READY: c_int = 4;
pub(super) const CONTEXT_FAILED: c_int = 5;
pub(super) const CONTEXT_TERMINATED: c_int = 6;
pub(super) const STREAM_READY: c_int = 2;
pub(super) const STREAM_FAILED: c_int = 3;
pub(super) const STREAM_TERMINATED: c_int = 4;
pub(super) const STREAM_TIMING: c_uint = 0x0002 | 0x0008 | 0x2000;
pub(super) const SEEK_RELATIVE: c_int = 0;
pub(super) const SAMPLE_FLOAT32LE: c_int = 5;

#[repr(C)]
pub(super) struct SampleSpec {
    pub(super) format: c_int,
    pub(super) rate: u32,
    pub(super) channels: u8,
}

#[repr(C)]
pub(super) struct BufferAttr {
    pub(super) maxlength: u32,
    pub(super) tlength: u32,
    pub(super) prebuf: u32,
    pub(super) minreq: u32,
    pub(super) fragsize: u32,
}

pub(super) type Loop = c_void;
pub(super) type Context = c_void;
pub(super) type Stream = c_void;
pub(super) type ContextNotify = unsafe extern "C" fn(context: *mut Context, data: *mut c_void);
pub(super) type StreamNotify = unsafe extern "C" fn(stream: *mut Stream, data: *mut c_void);
pub(super) type StreamRequest = unsafe extern "C" fn(stream: *mut Stream, bytes: usize, data: *mut c_void);

#[derive(Clone, Copy)]
pub(super) struct Api {
    pub(super) loop_new: unsafe extern "C" fn() -> *mut Loop,
    pub(super) loop_free: unsafe extern "C" fn(m: *mut Loop),
    pub(super) loop_start: unsafe extern "C" fn(m: *mut Loop) -> c_int,
    pub(super) loop_stop: unsafe extern "C" fn(m: *mut Loop),
    pub(super) lock: unsafe extern "C" fn(m: *mut Loop),
    pub(super) unlock: unsafe extern "C" fn(m: *mut Loop),
    pub(super) wait: unsafe extern "C" fn(m: *mut Loop),
    pub(super) signal: unsafe extern "C" fn(m: *mut Loop, wait_for_accept: c_int),
    pub(super) loop_api: unsafe extern "C" fn(m: *mut Loop) -> *mut c_void,
    pub(super) context_new: unsafe extern "C" fn(api: *mut c_void, name: *const c_char) -> *mut Context,
    pub(super) context_connect: unsafe extern "C" fn(c: *mut Context, server: *const c_char, flags: c_uint, spawn: *const c_void) -> c_int,
    pub(super) context_state: unsafe extern "C" fn(c: *mut Context) -> c_int,
    pub(super) context_on_state: unsafe extern "C" fn(c: *mut Context, cb: Option<ContextNotify>, data: *mut c_void),
    pub(super) context_disconnect: unsafe extern "C" fn(c: *mut Context),
    pub(super) context_unref: unsafe extern "C" fn(c: *mut Context),
    pub(super) context_errno: unsafe extern "C" fn(c: *mut Context) -> c_int,
    pub(super) stream_new: unsafe extern "C" fn(c: *mut Context, name: *const c_char, spec: *const SampleSpec, map: *const c_void) -> *mut Stream,
    pub(super) connect_playback: unsafe extern "C" fn(
        s: *mut Stream,
        dev: *const c_char,
        attr: *const BufferAttr,
        flags: c_uint,
        volume: *const c_void,
        sync: *mut Stream,
    ) -> c_int,
    pub(super) connect_record: unsafe extern "C" fn(s: *mut Stream, dev: *const c_char, attr: *const BufferAttr, flags: c_uint) -> c_int,
    pub(super) stream_state: unsafe extern "C" fn(s: *mut Stream) -> c_int,
    pub(super) on_state: unsafe extern "C" fn(s: *mut Stream, cb: Option<StreamNotify>, data: *mut c_void),
    pub(super) on_write: unsafe extern "C" fn(s: *mut Stream, cb: Option<StreamRequest>, data: *mut c_void),
    pub(super) on_read: unsafe extern "C" fn(s: *mut Stream, cb: Option<StreamRequest>, data: *mut c_void),
    pub(super) on_underflow: unsafe extern "C" fn(s: *mut Stream, cb: Option<StreamNotify>, data: *mut c_void),
    pub(super) on_started: unsafe extern "C" fn(s: *mut Stream, cb: Option<StreamNotify>, data: *mut c_void),
    pub(super) writable: unsafe extern "C" fn(s: *mut Stream) -> usize,
    pub(super) write: unsafe extern "C" fn(s: *mut Stream, data: *const c_void, bytes: usize, free: *const c_void, offset: i64, seek: c_int) -> c_int,
    pub(super) readable: unsafe extern "C" fn(s: *mut Stream) -> usize,
    pub(super) peek: unsafe extern "C" fn(s: *mut Stream, data: *mut *const c_void, bytes: *mut usize) -> c_int,
    pub(super) drop: unsafe extern "C" fn(s: *mut Stream) -> c_int,
    pub(super) stream_disconnect: unsafe extern "C" fn(s: *mut Stream) -> c_int,
    pub(super) stream_unref: unsafe extern "C" fn(s: *mut Stream),
    pub(super) strerror: unsafe extern "C" fn(error: c_int) -> *const c_char,
    pub(super) stream_time: unsafe extern "C" fn(s: *mut Stream, usec: *mut u64) -> c_int,
    pub(super) update_timing: unsafe extern "C" fn(s: *mut Stream, cb: *const c_void, data: *mut c_void) -> *mut c_void,
    pub(super) operation_unref: unsafe extern "C" fn(o: *mut c_void),
    pub(super) stream_ref: unsafe extern "C" fn(s: *mut Stream) -> *mut Stream,
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

pub(super) fn api() -> Result<Api, String> {
    API.get_or_init(|| {
        let handle = dlopen_library(c"libpulse.so.0")?;
        // SAFETY: `handle` is libpulse, and each field's type matches the symbol's C signature in `pulse/thread-mainloop.h`, `pulse/context.h`, `pulse/stream.h` and `pulse/error.h`.
        unsafe {
            Ok(Api {
                loop_new: dlsym_fn(handle, c"pa_threaded_mainloop_new")?,
                loop_free: dlsym_fn(handle, c"pa_threaded_mainloop_free")?,
                loop_start: dlsym_fn(handle, c"pa_threaded_mainloop_start")?,
                loop_stop: dlsym_fn(handle, c"pa_threaded_mainloop_stop")?,
                lock: dlsym_fn(handle, c"pa_threaded_mainloop_lock")?,
                unlock: dlsym_fn(handle, c"pa_threaded_mainloop_unlock")?,
                wait: dlsym_fn(handle, c"pa_threaded_mainloop_wait")?,
                signal: dlsym_fn(handle, c"pa_threaded_mainloop_signal")?,
                loop_api: dlsym_fn(handle, c"pa_threaded_mainloop_get_api")?,
                context_new: dlsym_fn(handle, c"pa_context_new")?,
                context_connect: dlsym_fn(handle, c"pa_context_connect")?,
                context_state: dlsym_fn(handle, c"pa_context_get_state")?,
                context_on_state: dlsym_fn(handle, c"pa_context_set_state_callback")?,
                context_disconnect: dlsym_fn(handle, c"pa_context_disconnect")?,
                context_unref: dlsym_fn(handle, c"pa_context_unref")?,
                context_errno: dlsym_fn(handle, c"pa_context_errno")?,
                stream_new: dlsym_fn(handle, c"pa_stream_new")?,
                connect_playback: dlsym_fn(handle, c"pa_stream_connect_playback")?,
                connect_record: dlsym_fn(handle, c"pa_stream_connect_record")?,
                stream_state: dlsym_fn(handle, c"pa_stream_get_state")?,
                on_state: dlsym_fn(handle, c"pa_stream_set_state_callback")?,
                on_write: dlsym_fn(handle, c"pa_stream_set_write_callback")?,
                on_read: dlsym_fn(handle, c"pa_stream_set_read_callback")?,
                on_underflow: dlsym_fn(handle, c"pa_stream_set_underflow_callback")?,
                on_started: dlsym_fn(handle, c"pa_stream_set_started_callback")?,
                writable: dlsym_fn(handle, c"pa_stream_writable_size")?,
                write: dlsym_fn(handle, c"pa_stream_write")?,
                readable: dlsym_fn(handle, c"pa_stream_readable_size")?,
                peek: dlsym_fn(handle, c"pa_stream_peek")?,
                drop: dlsym_fn(handle, c"pa_stream_drop")?,
                stream_disconnect: dlsym_fn(handle, c"pa_stream_disconnect")?,
                stream_unref: dlsym_fn(handle, c"pa_stream_unref")?,
                strerror: dlsym_fn(handle, c"pa_strerror")?,
                stream_time: dlsym_fn(handle, c"pa_stream_get_time")?,
                update_timing: dlsym_fn(handle, c"pa_stream_update_timing_info")?,
                operation_unref: dlsym_fn(handle, c"pa_operation_unref")?,
                stream_ref: dlsym_fn(handle, c"pa_stream_ref")?,
            })
        }
    })
    .clone()
}

pub(super) fn error_text(api: Api, code: c_int) -> String {
    // SAFETY: `pa_strerror` returns a static NUL-terminated string or null for any code.
    let text = unsafe { (api.strerror)(code) };
    if text.is_null() {
        format!("PulseAudio error {code}")
    } else {
        // SAFETY: `text` is non-null and NUL-terminated.
        unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
    }
}

use super::api::{self, Api, Context, Loop};
use std::ffi::c_void;
use std::sync::Arc;

pub(super) struct Wake {
    pub(super) api: Api,
    pub(super) mainloop: *mut Loop,
}

unsafe extern "C" fn wake_context(_context: *mut Context, data: *mut c_void) {
    // SAFETY: `data` is the connection's `Wake`, which outlives the context's callbacks; called with the lock held.
    let wake = unsafe { &*data.cast::<Wake>() };
    // SAFETY: as above.
    unsafe { (wake.api.signal)(wake.mainloop, 0) };
}

pub(super) struct Connection {
    wake: Box<Wake>,
    context: *mut Context,
    started: bool,
}

// SAFETY: every libpulse call on the loop and context happens with the threaded mainloop's lock held, which serializes them across threads; the pointers stay valid until `Drop`.
unsafe impl Send for Connection {}
// SAFETY: as for `Send`.
unsafe impl Sync for Connection {}

pub(super) struct Locked<'a>(&'a Connection);

impl Locked<'_> {
    pub(super) fn wait(&self) {
        // SAFETY: the lock is held by this guard, as `pa_threaded_mainloop_wait` requires.
        unsafe { (self.0.wake.api.wait)(self.0.wake.mainloop) };
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        // SAFETY: balances the lock this guard took.
        unsafe { (self.0.wake.api.unlock)(self.0.wake.mainloop) };
    }
}

impl Connection {
    pub(super) fn open(api: Api) -> Result<Arc<Self>, String> {
        // SAFETY: a new threaded mainloop and context; each is freed once in `Drop`.
        let mut connection = unsafe {
            let mainloop = (api.loop_new)();
            if mainloop.is_null() {
                return Err("PulseAudio couldn't make a mainloop".into());
            }
            let context = (api.context_new)((api.loop_api)(mainloop), c"Rune".as_ptr());
            if context.is_null() {
                (api.loop_free)(mainloop);
                return Err("PulseAudio couldn't make a context".into());
            }
            Self { wake: Box::new(Wake { api, mainloop }), context, started: false }
        };
        let wake: *mut Wake = &mut *connection.wake;
        // SAFETY: the loop isn't running yet, so nothing else touches the context; `wake` lives as long as the connection.
        unsafe {
            (api.context_on_state)(connection.context, Some(wake_context), wake.cast());
            if (api.context_connect)(connection.context, std::ptr::null(), 0, std::ptr::null()) < 0 {
                return Err(connection.error());
            }
            if (api.loop_start)(connection.wake.mainloop) < 0 {
                return Err("PulseAudio's mainloop didn't start".into());
            }
        }
        connection.started = true;
        let connection = Arc::new(connection);
        let locked = connection.lock();
        loop {
            // SAFETY: the lock is held.
            match unsafe { (api.context_state)(connection.context) } {
                api::CONTEXT_READY => break,
                api::CONTEXT_FAILED | api::CONTEXT_TERMINATED => return Err(connection.error()),
                _ => locked.wait(),
            }
        }
        drop(locked);
        Ok(connection)
    }

    pub(super) fn api(&self) -> Api {
        self.wake.api
    }

    pub(super) fn wake(&self) -> *const Wake {
        &*self.wake
    }

    pub(super) fn context(&self) -> *mut Context {
        self.context
    }

    pub(super) fn lock(&self) -> Locked<'_> {
        // SAFETY: the mainloop is live; the guard unlocks it.
        unsafe { (self.wake.api.lock)(self.wake.mainloop) };
        Locked(self)
    }

    pub(super) fn error(&self) -> String {
        // SAFETY: the context is live; its last error code is a plain read.
        api::error_text(self.wake.api, unsafe { (self.wake.api.context_errno)(self.context) })
    }

    pub(super) fn failed(&self) -> bool {
        // SAFETY: called with the lock held.
        matches!(unsafe { (self.wake.api.context_state)(self.context) }, api::CONTEXT_FAILED | api::CONTEXT_TERMINATED)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let api = self.wake.api;
        let locked = self.lock();
        // SAFETY: the lock is held; the context is disconnected and released once, after its callback is cleared.
        unsafe {
            (api.context_on_state)(self.context, None, std::ptr::null_mut());
            (api.context_disconnect)(self.context);
            (api.context_unref)(self.context);
        }
        drop(locked);
        // SAFETY: the loop is stopped without the lock held, then freed once.
        unsafe {
            if self.started {
                (api.loop_stop)(self.wake.mainloop);
            }
            (api.loop_free)(self.wake.mainloop);
        }
    }
}

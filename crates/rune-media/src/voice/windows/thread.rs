use crate::video::windows::mf::{symbol, system_library};
use std::sync::OnceLock;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::core::{BOOL, PCWSTR, w};

type SetCharacteristics = unsafe extern "system" fn(task: PCWSTR, index: *mut u32) -> HANDLE;
type Revert = unsafe extern "system" fn(handle: HANDLE) -> BOOL;

fn avrt() -> Option<(SetCharacteristics, Revert)> {
    static AVRT: OnceLock<Option<(SetCharacteristics, Revert)>> = OnceLock::new();
    *AVRT.get_or_init(|| {
        let module = system_library("avrt.dll")?;
        // SAFETY: the types are the documented signatures of these avrt.dll exports.
        unsafe { Some((symbol(module, c"AvSetMmThreadCharacteristicsW")?, symbol(module, c"AvRevertMmThreadCharacteristics")?)) }
    })
}

pub(super) struct Entered {
    com: bool,
    mmcss: Option<(HANDLE, Revert)>,
}

impl Entered {
    pub(super) fn apartment() -> Self {
        // SAFETY: joins this thread to the multithreaded apartment; `Drop` leaves it on the same thread.
        Self { com: unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok(), mmcss: None }
    }

    pub(super) fn audio_thread() -> Self {
        let mut entered = Self::apartment();
        entered.mmcss = avrt().and_then(|(set, revert)| {
            let mut index = 0u32;
            // SAFETY: registers this thread with the multimedia scheduler under a system task name.
            let handle = unsafe { set(w!("Pro Audio"), &mut index) };
            (!handle.is_invalid()).then_some((handle, revert))
        });
        entered
    }
}

impl Drop for Entered {
    fn drop(&mut self) {
        if let Some((handle, revert)) = self.mmcss.take() {
            // SAFETY: reverts the registration this thread made.
            unsafe { revert(handle) }.ok().ok();
        }
        if self.com {
            // SAFETY: balances the successful `CoInitializeEx` this thread made.
            unsafe { CoUninitialize() };
        }
    }
}

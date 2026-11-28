//! The one wait on the GPU every Rune crate uses (`rune_gpu::health` re-exports it): at the bottom
//! of Rune's crates, so `rune-gpu`, `rune-staging`'s readbacks and `rune-light` share it.

/// Whether `device`'s GPU was removed (a reset after a timeout, a driver update, a GPU
/// unplugged). D3D12 says so before wgpu does: Rune's wgpu-hal keeps a removed device's work
/// pending, never finished (`vendor/wgpu-hal`, `Device::reached`), and wgpu hears of the
/// removal only at the device's next submission. False on other backends, whose loss reaches
/// wgpu's lost callback.
#[allow(unsafe_code)]
pub fn removed(device: &wgpu::Device) -> bool {
    #[cfg(target_os = "windows")]
    {
        // SAFETY: the device's D3D12 interface, asked whether it was removed.
        unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }.is_some_and(|d| unsafe { d.raw_device().GetDeviceRemovedReason() }.is_err())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = device;
        false
    }
}

/// Why D3D12 removed a device, read when wgpu reports it lost (whose own reason is `Unknown`):
/// `DXGI_ERROR_DEVICE_HUNG` (a GPU timeout), `DXGI_ERROR_DEVICE_RESET`,
/// `DXGI_ERROR_DEVICE_REMOVED`, `DXGI_ERROR_DRIVER_INTERNAL_ERROR` or another `HRESULT`. Holds a
/// reference to the device's D3D12 interface, so keep it only as long as the `wgpu::Device`
/// (inside its lost callback). Reads nothing on other backends.
pub struct RemovalReason {
    #[cfg(target_os = "windows")]
    device: Option<RemovedDevice>,
}

#[cfg(target_os = "windows")]
struct RemovedDevice(windows::Win32::Graphics::Direct3D12::ID3D12Device);
#[cfg(target_os = "windows")]
// SAFETY: ID3D12Device is free-threaded; it is only asked for its removal reason.
#[allow(unsafe_code)]
unsafe impl Send for RemovedDevice {}
#[cfg(target_os = "windows")]
// SAFETY: as for Send.
#[allow(unsafe_code)]
unsafe impl Sync for RemovedDevice {}

impl RemovalReason {
    /// Watches `device`; on other backends it never reads a reason.
    #[allow(unsafe_code)]
    pub fn watch(device: &wgpu::Device) -> RemovalReason {
        #[cfg(target_os = "windows")]
        {
            // SAFETY: the device's D3D12 interface, cloned (a reference count) and only read.
            let device = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }.map(|d| RemovedDevice(d.raw_device().clone()));
            RemovalReason { device }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = device;
            RemovalReason {}
        }
    }

    /// The removal's reason, such as `DXGI_ERROR_DEVICE_HUNG (0x887A0006)`, or None while the
    /// device is not removed.
    #[allow(unsafe_code)]
    pub fn read(&self) -> Option<String> {
        #[cfg(target_os = "windows")]
        {
            let device = self.device.as_ref()?;
            // SAFETY: a live D3D12 device interface, asked whether it was removed.
            let error = unsafe { device.0.GetDeviceRemovedReason() }.err()?;
            Some(removal_name(error.code().0))
        }
        #[cfg(not(target_os = "windows"))]
        None
    }
}

#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) fn removal_name(code: i32) -> String {
    let name = match code as u32 {
        0x887A_0005 => "DXGI_ERROR_DEVICE_REMOVED",
        0x887A_0006 => "DXGI_ERROR_DEVICE_HUNG",
        0x887A_0007 => "DXGI_ERROR_DEVICE_RESET",
        0x887A_0020 => "DXGI_ERROR_DRIVER_INTERNAL_ERROR",
        0x887A_0001 => "DXGI_ERROR_INVALID_CALL",
        0x8007_000E => "E_OUTOFMEMORY",
        _ => return format!("0x{:08X}", code as u32),
    };
    format!("{name} (0x{:08X})", code as u32)
}

/// Why `poll` gave no status.
#[derive(Debug)]
pub enum PollFailed {
    /// The device is gone: nothing it was given will finish.
    Lost,
    /// The wait timed out, or named a submission the queue never had.
    Wait(wgpu::PollError),
}

impl std::fmt::Display for PollFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollFailed::Lost => f.write_str("the GPU device was lost"),
            PollFailed::Wait(error) => write!(f, "{error}"),
        }
    }
}

/// `Device::poll`, every Rune wait on the GPU goes through: a lost or removed device is an
/// error, never a wait that can't end and never a panic. wgpu 30 panics in `poll` once a fence
/// read fails ("Error in Device::poll: Parent device is lost"): on Vulkan that is how a lost
/// device shows. Until wgpu returns that as an error, the panic is caught here, and only that
/// one (wgpu holds no lock when it panics there; a callback's panic during the poll goes on).
/// On D3D12 the removal is asked first, so it never reaches wgpu's panic.
pub fn poll(device: &wgpu::Device, kind: wgpu::PollType) -> Result<wgpu::PollStatus, PollFailed> {
    if removed(device) {
        return Err(PollFailed::Lost);
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| device.poll(kind))) {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(_)) if removed(device) => Err(PollFailed::Lost),
        Ok(Err(error)) => Err(PollFailed::Wait(error)),
        Err(panic) => {
            let message = panic.downcast_ref::<String>().map(String::as_str).or_else(|| panic.downcast_ref::<&str>().copied()).unwrap_or("");
            if message.starts_with("Error in Device::poll") {
                return Err(PollFailed::Lost);
            }
            std::panic::resume_unwind(panic)
        }
    }
}

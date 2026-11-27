#![allow(unsafe_code)]

use rune::time::{Instant, SystemTime};
#[cfg(target_vendor = "apple")]
use std::time::{Duration, UNIX_EPOCH};

pub fn wall_of(at: Instant) -> SystemTime {
    let (now, wall) = (rune::time::now(), rune::time::system_now());
    if at <= now { wall - (now - at) } else { wall + (at - now) }
}

pub fn since_start_ms(at: SystemTime) -> Option<f64> {
    let start = process_start()?;
    Some(at.duration_since(start).ok()?.as_secs_f64() * 1000.0)
}

#[cfg(target_vendor = "apple")]
pub fn process_start() -> Option<SystemTime> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, std::process::id() as libc::c_int];
    let mut size: libc::size_t = 0;
    // SAFETY: a null buffer asks sysctl for the size of this process's `kinfo_proc`.
    let asked = unsafe { libc::sysctl(mib.as_mut_ptr(), 4, std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) };
    if asked != 0 || size < 16 { return None; }
    let mut buf = vec![0u64; size.div_ceil(8)];
    // SAFETY: `buf` holds `size` bytes, 8-byte aligned, which sysctl fills with a `kinfo_proc`.
    let read = unsafe { libc::sysctl(mib.as_mut_ptr(), 4, buf.as_mut_ptr().cast(), &mut size, std::ptr::null_mut(), 0) };
    if read != 0 { return None; }
    // SAFETY: `kinfo_proc` starts with `extern_proc`, whose first member is a union holding
    // `p_starttime`, a `timeval` (`time_t` seconds, then `suseconds_t` microseconds).
    let start = unsafe { std::ptr::read(buf.as_ptr().cast::<libc::timeval>()) };
    Some(UNIX_EPOCH + Duration::new(start.tv_sec as u64, start.tv_usec as u32 * 1000))
}

#[cfg(not(target_vendor = "apple"))]
pub fn process_start() -> Option<SystemTime> { None }

#[cfg(target_vendor = "apple")]
#[repr(C, packed(4))]
#[derive(Default)]
struct TaskVmInfo {
    virtual_size: u64,
    region_count: i32,
    page_size: i32,
    resident_size: u64,
    resident_size_peak: u64,
    device: u64,
    device_peak: u64,
    internal: u64,
    internal_peak: u64,
    external: u64,
    external_peak: u64,
    reusable: u64,
    reusable_peak: u64,
    purgeable_volatile_pmap: u64,
    purgeable_volatile_resident: u64,
    purgeable_volatile_virtual: u64,
    compressed: u64,
    compressed_peak: u64,
    compressed_lifetime: u64,
    phys_footprint: u64,
}

#[cfg(target_vendor = "apple")]
const TASK_VM_INFO: libc::task_flavor_t = 22;

// SAFETY: libSystem exports the task's own port as `mach_task_self_`, set before `main` runs.
#[cfg(target_vendor = "apple")]
unsafe extern "C" {
    static mach_task_self_: libc::mach_port_t;
}

#[cfg(target_vendor = "apple")]
pub fn footprint_bytes() -> Option<u64> {
    let mut info = TaskVmInfo::default();
    let mut count = (std::mem::size_of::<TaskVmInfo>() / std::mem::size_of::<libc::natural_t>()) as libc::mach_msg_type_number_t;
    // SAFETY: `info` is a `task_vm_info` through `phys_footprint` (TASK_VM_INFO_REV1_COUNT
    // naturals), and `count` says how many naturals it holds.
    let status = unsafe { libc::task_info(mach_task_self_, TASK_VM_INFO, (&mut info as *mut TaskVmInfo).cast(), &mut count) };
    (status == libc::KERN_SUCCESS).then_some(info.phys_footprint)
}

#[cfg(not(target_vendor = "apple"))]
pub fn footprint_bytes() -> Option<u64> { None }

pub fn footprint_mib() -> f64 { footprint_bytes().map_or(f64::NAN, |b| b as f64 / (1024.0 * 1024.0)) }

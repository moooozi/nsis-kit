//! Elevation boundary for the same-EXE installation worker.
pub(super) use super::launch::launch_frontend;
use super::{Stack, push};
use std::ffi::c_void;
use windows_sys::Win32::Foundation::HWND;

/// Installer script/plugin stack contract. Bump when argument order changes.
/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn InterfaceVersion(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size > 0 {
        push(stack, size as usize, "2");
    }
}

/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn IsElevated(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let elevated = if cfg!(feature = "fixture-engine") {
        std::env::var("NATIVE_INSTALLER_FIXTURE_BROKER").as_deref() != Ok("1")
            || std::env::args().any(|arg| arg == "/FIXTURE-ELEVATED")
    } else {
        super::launch::token_elevated().unwrap_or(false)
    };
    push(stack, size as usize, if elevated { "1" } else { "0" });
}

/// Build verifier rejects a fixture DLL even if Cargo feature unification enabled it.
/// # Safety
/// Marker export; never called by production NSIS.
#[cfg(feature = "fixture-engine")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn FixtureOnly(
    _: HWND,
    _: i32,
    _: *mut u16,
    _: *mut *mut Stack,
    _: *mut c_void,
) {
}

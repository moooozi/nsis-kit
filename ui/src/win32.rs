//! Small shared ownership primitives for the installer OS adapters.
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};

pub(super) struct Handle(pub HANDLE);
// Owned kernel handles can be transferred and closed on any process thread.
unsafe impl Send for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(super) fn error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}

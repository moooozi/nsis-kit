//! Explicit process/event synchronization supplied by a consumer hook.
use super::win32::{Handle, error};
use super::{Stack, pop, push, wide};
use std::{ffi::c_void, path::Path};
use windows_sys::Win32::{Foundation::*, System::Threading::*};

fn matching_path(actual: &[u16], expected: &Path) -> bool {
    String::from_utf16_lossy(actual).eq_ignore_ascii_case(&expected.to_string_lossy())
}

unsafe fn shutdown(executable: &Path, event_name: &str, pid: u32) -> Result<(), String> {
    if pid == 0 {
        return Err("The caller must supply an application process ID.".into());
    }
    let process = OpenProcess(
        PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZATION_SYNCHRONIZE,
        0,
        pid,
    );
    if process.is_null() {
        return Err(error("Cannot open application process"));
    }
    let process = Handle(process);
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    if QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut length) == 0 {
        return Err(error("Cannot identify application process"));
    }
    if !matching_path(&path[..length as usize], executable) {
        return Err("Process ID does not match the configured application.".into());
    }
    let event = OpenEventW(2, 0, wide(event_name).as_ptr());
    if event.is_null() {
        return Err(error("Cannot open application shutdown event"));
    }
    let event = Handle(event);
    if SetEvent(event.0) == 0 {
        return Err(error("Cannot request application shutdown"));
    }
    if WaitForSingleObject(process.0, 35000) != WAIT_OBJECT_0 {
        return Err(
            "The application did not finish shutdown. Save changes, close it and retry the update."
                .into(),
        );
    }
    Ok(())
}

/// # Safety
/// Unicode NSIS ABI; takes executable path, event name and process ID.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PrepareShutdown(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let executable = pop(stack, size as usize);
    let event_name = pop(stack, size as usize);
    let pid = pop(stack, size as usize);
    let result = match pid.parse::<u32>() {
        Ok(pid) => shutdown(Path::new(&executable), &event_name, pid),
        Err(_) => Err("Invalid shutdown PID.".into()),
    };
    push(stack, size as usize, &result.err().unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_matching_is_exact_not_prefix() {
        let path = Path::new(r"C:\Program Files\ExampleApp\example.exe");
        let actual = wide(r"c:\program files\exampleapp\EXAMPLE.exe");
        assert!(matching_path(&actual[..actual.len() - 1], path));
        assert!(!matching_path(&wide(r"C:\Other\example.exe"), path));
    }
}

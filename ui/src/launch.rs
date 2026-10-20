//! Original-user launch and elevation-token policy. Never launches an elevated GUI.
use super::win32::{Handle, error};
use super::{Stack, pop, push, wide};
use std::{ffi::c_void, path::Path, ptr::*};
use windows_sys::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};
pub(super) unsafe fn token_elevated() -> Result<bool, String> {
    use windows_sys::Win32::Security::*;
    let mut token = null_mut();
    if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
        return Err(error("Cannot query the Setup token"));
    }
    let token = Handle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut bytes = 0;
    if GetTokenInformation(
        token.0,
        TokenElevation,
        (&mut elevation as *mut TOKEN_ELEVATION).cast(),
        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
        &mut bytes,
    ) == 0
    {
        return Err(error("Cannot verify Setup elevation"));
    }
    Ok(elevation.TokenIsElevated != 0)
}

pub(super) unsafe fn launch_frontend(directory: &str) -> Result<(), String> {
    if token_elevated()? {
        launch(directory)
    } else {
        std::process::Command::new(Path::new(directory).join(super::branding::current().executable))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Unable to launch the application: {e}"))
    }
}

/// Launch through the interactive shell's primary token, never Setup's elevated token.
unsafe fn launch(directory: &str) -> Result<(), String> {
    use windows_sys::Win32::Security::*;
    let shell = GetShellWindow();
    let mut pid = 0;
    if shell.is_null() || GetWindowThreadProcessId(shell, &mut pid) == 0 {
        return Err(error("Cannot find original-user shell"));
    }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return Err(error("Cannot open original-user shell"));
    }
    let process = Handle(process);
    let mut token = null_mut();
    if OpenProcessToken(process.0, TOKEN_DUPLICATE | TOKEN_QUERY, &mut token) == 0 {
        return Err(error("Cannot open shell token"));
    }
    let token = Handle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut bytes = 0;
    if GetTokenInformation(
        token.0,
        TokenElevation,
        (&mut elevation as *mut TOKEN_ELEVATION).cast(),
        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
        &mut bytes,
    ) == 0
    {
        return Err(error("Cannot verify original-user token"));
    }
    if elevation.TokenIsElevated != 0 {
        return Err("The desktop shell is elevated. Refusing to launch the GUI elevated.".into());
    }
    let mut primary = null_mut();
    if DuplicateTokenEx(
        token.0,
        TOKEN_ALL_ACCESS,
        null(),
        SecurityImpersonation,
        TokenPrimary,
        &mut primary,
    ) == 0
    {
        return Err(error("Cannot duplicate shell token"));
    }
    let primary = Handle(primary);
    use windows_sys::Win32::System::Environment::{
        CreateEnvironmentBlock, DestroyEnvironmentBlock,
    };
    struct Environment(*mut c_void);
    impl Drop for Environment {
        fn drop(&mut self) {
            unsafe {
                DestroyEnvironmentBlock(self.0);
            }
        }
    }
    let mut environment = null_mut();
    if CreateEnvironmentBlock(&mut environment, primary.0, 0) == 0 {
        return Err(error("Cannot create original-user environment"));
    }
    let environment = Environment(environment);
    let executable = Path::new(directory).join(super::branding::current().executable);
    let mut command = wide(&format!("\"{}\"", executable.display()));
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..std::mem::zeroed()
    };
    let mut info = std::mem::zeroed::<PROCESS_INFORMATION>();
    if CreateProcessWithTokenW(
        primary.0,
        0,
        wide(&executable.to_string_lossy()).as_ptr(),
        command.as_mut_ptr(),
        CREATE_UNICODE_ENVIRONMENT,
        environment.0,
        wide(directory).as_ptr(),
        &startup,
        &mut info,
    ) == 0
    {
        return Err(error("Cannot launch the application as the desktop user"));
    }
    let _process = Handle(info.hProcess);
    let _thread = Handle(info.hThread);
    Ok(())
}

/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Launch(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let directory = pop(stack, size as usize);
    push(
        stack,
        size as usize,
        &launch(&directory).err().unwrap_or_default(),
    );
}

//! Unelevated presentation broker. Only the NSIS worker owns machine mutations.
use super::*;
use windows_sys::Win32::System::{DataExchange::COPYDATASTRUCT, Threading::*};

const PROGRESS_MESSAGE: usize = 0x53425700;

#[cfg(all(test, feature = "installer"))]
mod tests {
    use super::*;

    #[test]
    fn early_progress_is_buffered_and_cannot_complete_an_operation() {
        let engine = Engine::new(false, String::new());
        engine.pending.set(true);
        let status = wide("Configuring application integrations...");
        let data = COPYDATASTRUCT {
            dwData: PROGRESS_MESSAGE + 85,
            cbData: (status.len() * 2) as u32,
            lpData: status.as_ptr().cast_mut().cast(),
        };
        assert!(!unsafe { engine.receive(77, &data) });
        assert_eq!(engine.pending_report.borrow().as_ref().unwrap().0, 77);
        engine.pid.set(77);
        assert!(!unsafe { engine.receive(78, &data) });
        assert!(unsafe { engine.receive(77, &data) });
        assert_eq!(engine.progress.get(), 85);
        assert!(!engine.finished.get());
        assert_ne!(engine.code.get(), 0);
    }

    #[test]
    fn progress_rejects_malformed_or_unbounded_messages() {
        let engine = Engine::new(false, String::new());
        engine.pending.set(true);
        engine.pid.set(77);
        let mut data = COPYDATASTRUCT {
            dwData: PROGRESS_MESSAGE,
            cbData: 2,
            lpData: null_mut(),
        };
        assert!(!unsafe { engine.receive(77, &data) });
        let status = wide("test");
        data.lpData = status.as_ptr().cast_mut().cast();
        for bytes in [0, 1, 4098] {
            data.cbData = bytes;
            assert!(!unsafe { engine.receive(77, &data) });
        }
        data.cbData = (status.len() * 2) as u32;
        data.dwData = PROGRESS_MESSAGE + 101;
        assert!(!unsafe { engine.receive(77, &data) });
        assert!(engine.status.borrow().is_empty());
    }
}

pub(super) struct Engine {
    process: Cell<HANDLE>,
    pid: Cell<u32>,
    pending: Cell<bool>,
    pending_report: RefCell<Option<(usize, usize, String)>>,
    restore_foreground: Cell<bool>,
    pub finished: Cell<bool>,
    pub code: Cell<u32>,
    pub progress: Cell<u32>,
    pub status: RefCell<String>,
    pub action: Cell<&'static str>,
    restart: bool,
    worker_arguments: String,
}

impl Engine {
    #[cfg(feature = "installer")]
    fn new(restart: bool, worker_arguments: String) -> Self {
        Self {
            process: Cell::new(null_mut()),
            pid: Cell::new(0),
            pending: Cell::new(false),
            pending_report: RefCell::new(None),
            restore_foreground: Cell::new(false),
            finished: Cell::new(false),
            code: Cell::new(1),
            progress: Cell::new(0),
            status: RefCell::new(String::new()),
            action: Cell::new("install"),
            restart,
            worker_arguments,
        }
    }
    pub fn active(&self) -> bool {
        self.pending.get() || !self.process.get().is_null()
    }
    pub fn started(&self) -> bool {
        self.active() || self.finished.get()
    }
    pub unsafe fn start(&self, window: HWND, action: &'static str, directory: &str, ui: &Ui) {
        let restore_foreground = GetForegroundWindow() == window;
        self.restore_foreground.set(restore_foreground);
        for name in [
            "NativeInstaller.NsisFocusBeforePID",
            "NativeInstaller.NsisFocusGrant",
            "NativeInstaller.NsisFocusResult",
            "NativeInstaller.NsisFocusFrontendResult",
            "NativeInstaller.NsisFocusHandoffs",
        ] {
            RemovePropW(window, wide(name).as_ptr());
        }
        self.action.set(action);
        self.pending.set(true);
        *self.status.borrow_mut() = "Waiting for Windows permission…".into();
        // Paint the transition before ShellExecute enters the UAC modal loop.
        engine_controls(window, ui);
        InvalidateRect(window, null(), 0);
        UpdateWindow(window);
        let args = format!(
            "/S /WORKER /ACTION={action} /UIHWND={} {} {} {} /D={directory}",
            window as usize,
            self.worker_arguments,
            if self.restart { "/RESTARTAPP" } else { "" },
            if restore_foreground {
                "/ACTIVATEUI"
            } else {
                ""
            }
        );
        let result = std::env::current_exe()
            .map_err(|_| ERROR_FILE_NOT_FOUND)
            .and_then(|exe| execute(window, &exe.to_string_lossy(), &args, SW_HIDE));
        // A successful launch hands foreground permission to the worker. It must
        // explicitly hand it back; the unelevated caller cannot reclaim it alone.
        if restore_foreground && result.is_err() {
            SetForegroundWindow(window);
        }
        self.pending.set(false);
        self.finish_foreground_handoff(window);
        match result {
            Ok(process) => {
                self.pid.set(GetProcessId(process));
                SetPropW(
                    window,
                    wide("NativeInstaller.NsisWorkerPID").as_ptr(),
                    self.pid.get() as usize as HANDLE,
                );
                self.process.set(process);
                // ShellExecute may pump early worker reports before returning its
                // process handle. Authenticate the last one only now, by PID.
                if let Some((sender, progress, status)) = self.pending_report.borrow_mut().take()
                    && sender == self.pid.get() as usize
                {
                    *self.status.borrow_mut() = status;
                    if progress <= 100 {
                        self.progress.set(progress as u32);
                    }
                } else {
                    *self.status.borrow_mut() = "Preparing installation…".into();
                }
                SetTimer(window, 1, 100, None);
            }
            Err(ERROR_CANCELLED) => {
                *self.status.borrow_mut() =
                    "Windows permission was canceled. No changes were made; you can try again."
                        .into()
            }
            Err(code) => {
                *self.status.borrow_mut() = format!(
                    "Unable to start Setup: {}",
                    std::io::Error::from_raw_os_error(code as i32)
                )
            }
        }
    }
    pub unsafe fn finish_foreground_handoff(&self, window: HWND) {
        if self.restore_foreground.get()
            && IsWindowEnabled(window) != 0
            && GetPropW(window, wide("NativeInstaller.NsisFocusGrant").as_ptr()) as usize == 1
        {
            // The worker can grant permission while ShellExecute still has this
            // window disabled. Consume that grant only after the modal handoff.
            self.restore_foreground.set(false);
            let restored = GetForegroundWindow() == window || SetForegroundWindow(window) != 0;
            SetPropW(
                window,
                wide("NativeInstaller.NsisFocusFrontendResult").as_ptr(),
                (if restored { 1 } else { 2 }) as HANDLE,
            );
        }
    }

    pub unsafe fn poll(&self, window: HWND) -> bool {
        let process = self.process.get();
        if process.is_null() || WaitForSingleObject(process, 0) != WAIT_OBJECT_0 {
            return false;
        }
        let mut code = 2;
        GetExitCodeProcess(process, &mut code);
        CloseHandle(self.process.replace(null_mut()));
        self.code.set(code);
        self.finished.set(true);
        if code == 0 {
            self.progress.set(100);
            *self.status.borrow_mut() = if self.action.get() == "uninstall" {
                format!("{} has been uninstalled.", branding::current().name)
            } else {
                branding::current().installed_message
            };
        } else if !self.status.borrow().starts_with("Error: ") {
            *self.status.borrow_mut() = format!(
                "Setup did not complete (exit code {code}). No automatic restart was attempted. Run Setup again after resolving the problem."
            );
        }
        KillTimer(window, 1);
        true
    }
    pub unsafe fn receive(&self, sender: usize, data: &COPYDATASTRUCT) -> bool {
        if sender == 0
            || !self.active()
            || data.dwData < PROGRESS_MESSAGE
            || data.dwData > PROGRESS_MESSAGE + 1000
            || data.cbData < 2
            || data.cbData > 4096
            || !data.cbData.is_multiple_of(2)
            || data.lpData.is_null()
        {
            return false;
        }
        let progress = data.dwData - PROGRESS_MESSAGE;
        if progress > 100 && progress != 1000 {
            return false;
        }
        let value = std::slice::from_raw_parts(data.lpData.cast::<u16>(), data.cbData as usize / 2);
        if value.last() != Some(&0) {
            return false;
        }
        let status = String::from_utf16_lossy(&value[..value.len() - 1]);
        if self.pending.get() && self.pid.get() == 0 {
            *self.pending_report.borrow_mut() = Some((sender, progress, status));
            return false;
        }
        if sender != self.pid.get() as usize {
            return false;
        }
        *self.status.borrow_mut() = status;
        if progress <= 100 {
            self.progress.set(progress as u32);
        }
        true
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        if !self.process.get().is_null() {
            unsafe {
                CloseHandle(self.process.get());
            }
        }
    }
}

/// Start this same executable through Windows' elevation broker. The fixture
/// build has a separate target directory and NEVER requests elevation.
unsafe fn execute(parent: HWND, executable: &str, args: &str, show: i32) -> Result<HANDLE, u32> {
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }
    let _apartment = Apartment(CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) >= 0);
    #[cfg(feature = "fixture-engine")]
    if std::env::var("NATIVE_INSTALLER_FIXTURE_UAC").as_deref() == Ok("cancel") {
        return Err(ERROR_CANCELLED);
    }
    let verb = wide(if cfg!(feature = "fixture-engine") {
        "open"
    } else {
        "runas"
    });
    let executable = wide(executable);
    #[cfg(feature = "fixture-engine")]
    let fixture_args = if std::env::var("NATIVE_INSTALLER_FIXTURE_BROKER").as_deref() == Ok("1") {
        format!("/FIXTURE-ELEVATED {args}")
    } else {
        args.to_owned()
    };
    #[cfg(feature = "fixture-engine")]
    let args = fixture_args.as_str();
    let args = wide(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        hwnd: parent,
        lpVerb: verb.as_ptr(),
        lpFile: executable.as_ptr(),
        lpParameters: args.as_ptr(),
        nShow: show,
        ..std::mem::zeroed()
    };
    if ShellExecuteExW(&mut info) == 0 {
        return Err(GetLastError());
    }
    if info.hProcess.is_null() {
        return Err(ERROR_INVALID_HANDLE);
    }
    Ok(info.hProcess)
}

/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[cfg(feature = "installer")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Run(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let mode = pop(stack, size as usize);
    let directory = pop(stack, size as usize);
    let restart = pop(stack, size as usize) == "1";
    let worker_arguments = pop(stack, size as usize);
    let code = match show(
        directory,
        "@103",
        false,
        true,
        mode == "maintenance",
        true,
        Some(Engine::new(restart, worker_arguments)),
    ) {
        Ok(Some(code)) => code,
        Ok(None) => "1".into(),
        Err(_) => "2".into(),
    };
    push(stack, size as usize, &code);
}

/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[cfg(feature = "installer")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Execute(
    parent: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let executable = pop(stack, size as usize);
    let args = pop(stack, size as usize);
    let (message, code) = match execute(parent, &executable, &args, SW_SHOWNORMAL) {
        Ok(process) => {
            let wait = WaitForSingleObject(process, INFINITE);
            let mut code = 2;
            let success = wait == WAIT_OBJECT_0 && GetExitCodeProcess(process, &mut code) != 0;
            CloseHandle(process);
            (
                if success {
                    String::new()
                } else {
                    "Unable to observe elevated Setup.".into()
                },
                code,
            )
        }
        Err(ERROR_CANCELLED) => (String::new(), 1),
        Err(code) => (
            format!(
                "Unable to start elevated Setup: {}",
                std::io::Error::from_raw_os_error(code as i32)
            ),
            2,
        ),
    };
    push(stack, size as usize, &code.to_string());
    push(stack, size as usize, &message);
}

/// Hand foreground permission back from the newly elevated worker, once.
/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[cfg(feature = "installer")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn HandoffForeground(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let window = pop(stack, size as usize).parse::<usize>().unwrap_or(0) as HWND;
    if window.is_null() {
        return;
    }
    let mut class = [0u16; 64];
    let count = GetClassNameW(window, class.as_mut_ptr(), class.len() as i32);
    if count <= 0 || String::from_utf16_lossy(&class[..count as usize]) != "NativeNSISInstaller" {
        return;
    }
    let mut pid = 0;
    GetWindowThreadProcessId(window, &mut pid);
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
        return;
    }
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    let matches = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) != 0
        && std::env::current_exe().is_ok_and(|exe| {
            String::from_utf16_lossy(&path[..length as usize])
                .eq_ignore_ascii_case(&exe.to_string_lossy())
        });
    CloseHandle(process);
    if !matches {
        return;
    }
    let count = GetPropW(window, wide("NativeInstaller.NsisFocusHandoffs").as_ptr()) as usize;
    SetPropW(
        window,
        wide("NativeInstaller.NsisFocusHandoffs").as_ptr(),
        (count + 1) as HANDLE,
    );
    let mut before = 0;
    GetWindowThreadProcessId(GetForegroundWindow(), &mut before);
    let granted = AllowSetForegroundWindow(pid) != 0;
    let activated = SetForegroundWindow(window) != 0;
    SetPropW(
        window,
        wide("NativeInstaller.NsisFocusBeforePID").as_ptr(),
        before as usize as HANDLE,
    );
    SetPropW(
        window,
        wide("NativeInstaller.NsisFocusGrant").as_ptr(),
        (if granted { 1 } else { 2 }) as HANDLE,
    );
    SetPropW(
        window,
        wide("NativeInstaller.NsisFocusResult").as_ptr(),
        (if activated { 1 } else { 2 }) as HANDLE,
    );
}

/// Only bounded presentation messages cross from the elevated worker. Completion
/// is determined independently from its process handle, never from these messages.
/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[cfg(feature = "installer")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Report(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let window = pop(stack, size as usize).parse::<usize>().unwrap_or(0) as HWND;
    let progress = pop(stack, size as usize).parse::<usize>().unwrap_or(1000);
    let status = pop(stack, size as usize);
    if window.is_null() || (progress > 100 && progress != 1000) {
        return;
    }
    let value = wide(&status);
    if value.len() > 2048 {
        return;
    }
    let data = COPYDATASTRUCT {
        dwData: PROGRESS_MESSAGE + progress,
        cbData: (value.len() * 2) as u32,
        lpData: value.as_ptr().cast_mut().cast(),
    };
    let mut result = 0;
    SendMessageTimeoutW(
        window,
        WM_COPYDATA,
        GetCurrentProcessId() as usize,
        (&data as *const COPYDATASTRUCT) as isize,
        SMTO_ABORTIFHUNG | SMTO_BLOCK,
        1000,
        &mut result,
    );
}

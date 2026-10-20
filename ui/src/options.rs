//! Windows argument tokenization only; installer policy belongs to NSIS.
use super::win32::error;
use super::{Stack, pop, push, wide};
use std::ffi::c_void;
use windows_sys::Win32::{Foundation::*, UI::Shell::CommandLineToArgvW};

unsafe fn tokenize(raw: &str) -> Result<Vec<String>, String> {
    let mut count = 0;
    let argv = CommandLineToArgvW(wide(&format!("setup.exe {raw}")).as_ptr(), &mut count);
    if argv.is_null() {
        return Err(error("Cannot parse installer arguments"));
    }
    let args = std::slice::from_raw_parts(argv, count as usize)
        .iter()
        .skip(1)
        .map(|p| {
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(*p, len))
        })
        .collect();
    LocalFree(argv.cast());
    Ok(args)
}

// Quote each opaque consumer token using Windows' backslash/quote rules. Never
// allow an extension to replace the runtime's routing, UI or destination flags.
unsafe fn worker_arguments(raw: &str, directory: &str, capacity: usize) -> Result<String, String> {
    let args = tokenize(raw)?;
    let mut output = Vec::new();
    for arg in args {
        let upper = arg.to_ascii_uppercase();
        if ["/S", "/WORKER", "/AUTOMATIC", "/RESTARTAPP", "/ACTIVATEUI"].contains(&upper.as_str())
            || ["/ACTION=", "/UIHWND=", "/D=", "_?="]
                .iter()
                .any(|prefix| upper.starts_with(prefix))
        {
            return Err("Consumer arguments contain a reserved installer option.".into());
        }
        let mut quoted = String::from("\"");
        let mut slashes = 0;
        for ch in arg.chars() {
            if ch == '\\' {
                slashes += 1;
                continue;
            }
            quoted.extend(std::iter::repeat_n(
                '\\',
                if ch == '"' { slashes * 2 + 1 } else { slashes },
            ));
            quoted.push(ch);
            slashes = 0;
        }
        quoted.extend(std::iter::repeat_n('\\', slashes * 2));
        quoted.push('"');
        output.push(quoted);
    }
    let output = output.join(" ");
    // Leave room for every toolkit flag, a decimal HWND and terminal /D=.
    // NSIS concatenation otherwise silently truncates the resulting command.
    if output.encode_utf16().count() + directory.encode_utf16().count() + 192 >= capacity {
        return Err("Consumer worker arguments exceed the installer command limit.".into());
    }
    Ok(output)
}

/// Takes consumer arguments and destination; returns canonical arguments, error.
/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn NormalizeWorkerArguments(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let raw = pop(stack, size as usize);
    let directory = pop(stack, size as usize);
    match worker_arguments(&raw, &directory, size as usize) {
        Ok(args) => {
            push(stack, size as usize, &args);
            push(stack, size as usize, "");
        }
        Err(message) => {
            push(stack, size as usize, "");
            push(stack, size as usize, &message);
        }
    }
}

/// Pop raw arguments. Push arguments in reverse order, count, then error.
/// Callers must drain exactly count arguments, even when they stop interpreting.
/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Tokenize(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let raw = pop(stack, size as usize);
    match tokenize(&raw) {
        Ok(args) => {
            for arg in args.iter().rev() {
                push(stack, size as usize, arg);
            }
            push(stack, size as usize, &args.len().to_string());
            push(stack, size as usize, "");
        }
        Err(message) => {
            push(stack, size as usize, "0");
            push(stack, size as usize, &message);
        }
    }
}

/// # Safety
/// Called only through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn UninstallOptions(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let silent = std::env::args()
        .skip(1)
        .take_while(|arg| !arg.starts_with("_?="))
        .any(|arg| arg == "/S");
    push(stack, size as usize, if silent { "1" } else { "0" });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;

    #[test]
    fn worker_arguments_round_trip_opaque_values_and_reject_reserved_options() {
        unsafe {
            let raw = r#"/CUSTOM "text with spaces" "quote\"value" "C:\folder\\" """#;
            let normalized = worker_arguments(raw, r"C:\Example", 1024).unwrap();
            assert_eq!(tokenize(raw).unwrap(), tokenize(&normalized).unwrap());
            for reserved in [
                "/WORKER",
                "/S",
                "/automatic",
                "/RESTARTAPP",
                "/ACTIVATEUI",
                "/ACTION=uninstall",
                "/UIHWND=123",
                "/D=C:\\Other",
                "_?=C:\\Other",
            ] {
                assert!(worker_arguments(reserved, r"C:\Example", 1024).is_err());
            }
            assert!(worker_arguments(&"x".repeat(900), r"C:\Example", 1024).is_err());
        }
    }

    #[test]
    fn tokenizer_preserves_unicode_quotes_and_stack_order_without_option_policy() {
        unsafe {
            let mut stack = null_mut();
            push(&mut stack, 1024, r#"/CUSTOM "中文 text" /BOUNDARY /WORKER"#);
            Tokenize(null_mut(), 1024, null_mut(), &mut stack, null_mut());
            assert_eq!(pop(&mut stack, 1024), "");
            assert_eq!(pop(&mut stack, 1024), "4");
            for expected in ["/CUSTOM", "中文 text", "/BOUNDARY", "/WORKER"] {
                assert_eq!(pop(&mut stack, 1024), expected);
            }
            assert!(stack.is_null());
        }
    }
}

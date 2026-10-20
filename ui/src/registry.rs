//! Bounded REG_MULTI_SZ transport through the NSIS stack. No product policy.
use super::{Stack, pop, push, wide};
use std::{ffi::c_void, ptr};
use windows_sys::Win32::{
    Foundation::{ERROR_SUCCESS, HWND},
    System::Registry::*,
};

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

fn root(hive: &str) -> Result<HKEY, String> {
    match hive {
        "HKLM:64" => Ok(HKEY_LOCAL_MACHINE),
        "HKCU:64" => Ok(HKEY_CURRENT_USER),
        _ => Err("Unsupported registry hive/view.".into()),
    }
}

fn decode(data: &[u16], capacity: usize) -> Result<Vec<String>, String> {
    if data.len() < 2 || !data.ends_with(&[0, 0]) {
        return Err("Registry string list is not terminated.".into());
    }
    if data == [0, 0] {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for value in data[..data.len() - 1].split_inclusive(|c| *c == 0) {
        let value = &value[..value.len() - 1];
        if value.is_empty() || value.len() >= capacity || result.len() >= MAX_ENTRIES {
            return Err("Registry string list contains an empty or oversized entry.".into());
        }
        result.push(
            String::from_utf16(value).map_err(|_| "Registry string list has invalid UTF-16.")?,
        );
    }
    Ok(result)
}

unsafe fn read(hive: &str, path: &str, name: &str, capacity: usize) -> Result<Vec<String>, String> {
    let mut key = ptr::null_mut();
    let status = RegOpenKeyExW(
        root(hive)?,
        wide(path).as_ptr(),
        0,
        KEY_READ | KEY_WOW64_64KEY,
        &mut key,
    );
    if status != ERROR_SUCCESS {
        return Err(format!(
            "Cannot open registry string list (error {status})."
        ));
    }
    let key = Key(key);
    let mut kind = 0;
    let mut bytes = 0;
    let status = RegQueryValueExW(
        key.0,
        wide(name).as_ptr(),
        ptr::null(),
        &mut kind,
        ptr::null_mut(),
        &mut bytes,
    );
    if status != ERROR_SUCCESS
        || kind != REG_MULTI_SZ
        || bytes as usize > MAX_BYTES
        || bytes % 2 != 0
    {
        return Err("Registry string list is missing, has the wrong type or is too large.".into());
    }
    let mut data = vec![0u16; bytes as usize / 2];
    let status = RegQueryValueExW(
        key.0,
        wide(name).as_ptr(),
        ptr::null(),
        &mut kind,
        data.as_mut_ptr().cast(),
        &mut bytes,
    );
    if status != ERROR_SUCCESS
        || kind != REG_MULTI_SZ
        || bytes as usize > data.len() * 2
        || bytes % 2 != 0
    {
        return Err("Registry string list changed while being read.".into());
    }
    data.truncate(bytes as usize / 2);
    decode(&data, capacity)
}

unsafe fn write(hive: &str, path: &str, name: &str, values: &[String]) -> Result<(), String> {
    let mut data = Vec::new();
    for value in values {
        if value.is_empty() || value.contains('\0') {
            return Err("Registry string list entries must not be empty.".into());
        }
        data.extend(value.encode_utf16());
        data.push(0);
        if data.len() * 2 >= MAX_BYTES {
            return Err("Registry string list is too large.".into());
        }
    }
    if data.is_empty() {
        data.push(0);
    }
    data.push(0);
    let mut key = ptr::null_mut();
    let status = RegOpenKeyExW(
        root(hive)?,
        wide(path).as_ptr(),
        0,
        KEY_SET_VALUE | KEY_WOW64_64KEY,
        &mut key,
    );
    if status != ERROR_SUCCESS {
        return Err(format!(
            "Cannot open registry string list for writing (error {status})."
        ));
    }
    let key = Key(key);
    let status = RegSetValueExW(
        key.0,
        wide(name).as_ptr(),
        0,
        REG_MULTI_SZ,
        data.as_ptr().cast(),
        (data.len() * 2) as u32,
    );
    if status != ERROR_SUCCESS {
        return Err(format!(
            "Cannot write registry string list (error {status})."
        ));
    }
    Ok(())
}

/// Input: hive/view, key, value name. Output: entries (first on top), count, error.
/// # Safety
/// Called through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReadRegistryList(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let capacity = size as usize;
    let hive = pop(stack, capacity);
    let path = pop(stack, capacity);
    let name = pop(stack, capacity);
    match read(&hive, &path, &name, capacity) {
        Ok(values) => {
            for value in values.iter().rev() {
                push(stack, capacity, value);
            }
            push(stack, capacity, &values.len().to_string());
            push(stack, capacity, "");
        }
        Err(message) => {
            push(stack, capacity, "0");
            push(stack, capacity, &message);
        }
    }
}

/// Caller pushes entries in source order. Input: hive/view, key, name, count.
/// Output: error. Entries are drained before writing.
/// # Safety
/// Called through the Unicode NSIS stack ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn WriteRegistryList(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let capacity = size as usize;
    let hive = pop(stack, capacity);
    let path = pop(stack, capacity);
    let name = pop(stack, capacity);
    let count = pop(stack, capacity).parse::<usize>();
    let result = (|| {
        let count = count.map_err(|_| "Invalid registry string list count.")?;
        if count > MAX_ENTRIES {
            return Err("Registry string list has too many entries.".into());
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(pop(stack, capacity));
        }
        values.reverse();
        write(&hive, &path, &name, &values)
    })();
    push(stack, capacity, &result.err().unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_unicode_lists_without_truncation() {
        let data: Vec<u16> = "resources\\\u{4e2d}\u{6587}.ico\0example.exe\0\0"
            .encode_utf16()
            .collect();
        assert_eq!(
            decode(&data, 1024).unwrap(),
            ["resources\\\u{4e2d}\u{6587}.ico", "example.exe"]
        );
        assert!(decode(&[0, 0], 1024).unwrap().is_empty());
    }

    #[test]
    fn rejects_malformed_or_truncated_lists() {
        for data in [
            vec![],
            vec![0],
            vec![65, 0],
            vec![65, 0, 0, 66, 0, 0],
            vec![0xd800, 0, 0],
        ] {
            assert!(decode(&data, 1024).is_err(), "{data:?}");
        }
        assert!(decode(&[65, 66, 0, 0], 2).is_err());
    }
}

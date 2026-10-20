//! Native button interaction and folder selection; no installer mutations.
use super::*;

fn hover_target(current: HWND, window: HWND, message: u32) -> HWND {
    match message {
        WM_MOUSEMOVE => window,
        WM_MOUSELEAVE if current == window => null_mut(),
        _ => current,
    }
}

pub(super) unsafe extern "system" fn button_proc(
    window: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    if msg == WM_ERASEBKGND {
        return 1;
    }
    let ui = &*(data as *const Ui);
    let previous = ui.hovered.get();
    let next = hover_target(previous, window, msg);
    if next != previous {
        ui.hovered.set(next);
        if !previous.is_null() {
            InvalidateRect(previous, null(), 0);
        }
        if !next.is_null() {
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: window,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tracking);
            InvalidateRect(window, null(), 0);
        }
    } else if msg == WM_SETFOCUS || msg == WM_KILLFOCUS {
        InvalidateRect(window, null(), 0);
    }
    if msg == WM_MOUSEMOVE && GetCapture() != window {
        // Owner drawing owns hover. The native button otherwise invalidates on
        // every move; still forward captured moves for press/drag cancellation.
        return 0;
    }
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(window, Some(button_proc), 1);
    }
    DefSubclassProc(window, msg, w, l)
}

unsafe extern "system" fn browse_callback(
    window: HWND,
    msg: u32,
    _: LPARAM,
    initial: LPARAM,
) -> i32 {
    if msg == BFFM_INITIALIZED {
        SendMessageW(window, BFFM_SETSELECTIONW, 1, initial);
    }
    0
}

pub(super) unsafe fn browse(window: HWND, ui: &Ui) {
    let mut path = [0u16; 1024];
    GetWindowTextW(ui.edit.get(), path.as_mut_ptr(), path.len() as i32);
    let title = wide("Choose a preview installation folder (nothing will be installed here)");
    let info = BROWSEINFOW {
        hwndOwner: window,
        pidlRoot: null_mut(),
        pszDisplayName: null_mut(),
        lpszTitle: title.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE | BIF_EDITBOX,
        lpfn: Some(browse_callback),
        lParam: path.as_ptr() as isize,
        iImage: 0,
    };
    let selected = SHBrowseForFolderW(&info);
    if !selected.is_null() {
        let mut output = [0u16; 260];
        if SHGetPathFromIDListW(selected, output.as_mut_ptr()) != 0 {
            let len = output.iter().position(|c| *c == 0).unwrap_or(output.len());
            let selected_path = String::from_utf16_lossy(&output[..len]);
            let selected_path = Path::new(&selected_path);
            let destination = if selected_path
                .file_name()
                .is_some_and(|v| v.eq_ignore_ascii_case(&ui.branding.name))
            {
                selected_path.to_owned()
            } else {
                selected_path.join(&ui.branding.name)
            };
            SetWindowTextW(ui.edit.get(), wide(&destination.to_string_lossy()).as_ptr());
        }
        CoTaskMemFree(selected.cast());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stationary_moves_do_not_transition_and_stale_leave_does_not_clear_another_button() {
        let a = 1u8;
        let b = 2u8;
        let first = (&a as *const u8).cast_mut().cast();
        let second = (&b as *const u8).cast_mut().cast();
        assert_eq!(hover_target(null_mut(), first, WM_MOUSEMOVE), first);
        for _ in 0..100 {
            assert_eq!(hover_target(first, first, WM_MOUSEMOVE), first);
        }
        assert_eq!(hover_target(first, second, WM_MOUSEMOVE), second);
        assert_eq!(hover_target(second, first, WM_MOUSELEAVE), second);
        assert!(hover_target(second, second, WM_MOUSELEAVE).is_null());
    }
}

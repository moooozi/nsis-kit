//! Compose a complete frame before exposing it; never erase a live button first.
use std::ptr::null_mut;
use windows_sys::Win32::{Foundation::RECT, Graphics::Gdi::*};

pub struct BufferedDc {
    target: HDC,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    width: i32,
    height: i32,
}

impl BufferedDc {
    pub unsafe fn new(target: HDC, bounds: RECT) -> Self {
        unsafe {
            let width = bounds.right - bounds.left;
            let height = bounds.bottom - bounds.top;
            let dc = CreateCompatibleDC(target);
            let bitmap = CreateCompatibleBitmap(target, width, height);
            if dc.is_null() || bitmap.is_null() {
                if !dc.is_null() {
                    DeleteDC(dc);
                }
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                return Self {
                    target,
                    dc: target,
                    bitmap: null_mut(),
                    previous: null_mut(),
                    width,
                    height,
                };
            }
            let previous = SelectObject(dc, bitmap);
            Self {
                target,
                dc,
                bitmap,
                previous,
                width,
                height,
            }
        }
    }

    pub fn dc(&self) -> HDC {
        self.dc
    }
}

/// Supersample only geometry. Text is drawn afterwards at device resolution.
pub unsafe fn rounded(target: HDC, rect: RECT, fill: u32, edge: u32, radius: i32, background: u32) {
    unsafe {
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let dc = CreateCompatibleDC(target);
        let bitmap = CreateCompatibleBitmap(target, width * 4, height * 4);
        if dc.is_null() || bitmap.is_null() {
            if !dc.is_null() {
                DeleteDC(dc);
            }
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            shape(target, rect, fill, edge, radius, 1);
            return;
        }
        let previous = SelectObject(dc, bitmap);
        let bounds = RECT {
            left: 0,
            top: 0,
            right: width * 4,
            bottom: height * 4,
        };
        let brush = CreateSolidBrush(background);
        FillRect(dc, &bounds, brush);
        DeleteObject(brush);
        shape(dc, bounds, fill, edge, radius * 4, 4);
        let mode = SetStretchBltMode(target, HALFTONE);
        let mut origin = std::mem::zeroed();
        SetBrushOrgEx(target, 0, 0, &mut origin);
        StretchBlt(
            target,
            rect.left,
            rect.top,
            width,
            height,
            dc,
            0,
            0,
            width * 4,
            height * 4,
            SRCCOPY,
        );
        SetBrushOrgEx(target, origin.x, origin.y, null_mut());
        SetStretchBltMode(target, mode);
        SelectObject(dc, previous);
        DeleteObject(bitmap);
        DeleteDC(dc);
    }
}

unsafe fn shape(dc: HDC, rect: RECT, fill: u32, edge: u32, radius: i32, pen_width: i32) {
    unsafe {
        let brush = CreateSolidBrush(fill);
        let pen = CreatePen(PS_SOLID, pen_width, edge);
        let old_brush = SelectObject(dc, brush);
        let old_pen = SelectObject(dc, pen);
        // Keep the full outline inside the image, including at fractional DPI.
        let inset = pen_width / 2;
        RoundRect(
            dc,
            rect.left + inset,
            rect.top + inset,
            rect.right - inset,
            rect.bottom - inset,
            radius,
            radius,
        );
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}

impl Drop for BufferedDc {
    fn drop(&mut self) {
        if !self.bitmap.is_null() {
            unsafe {
                BitBlt(
                    self.target,
                    0,
                    0,
                    self.width,
                    self.height,
                    self.dc,
                    0,
                    0,
                    SRCCOPY,
                );
                SelectObject(self.dc, self.previous);
                DeleteObject(self.bitmap);
                DeleteDC(self.dc);
            }
        }
    }
}

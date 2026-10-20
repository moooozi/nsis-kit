//! Compact layout in device-independent pixels. Only monitor DPI scales it.
use windows_sys::Win32::UI::WindowsAndMessaging::{IDCANCEL, IDOK};

pub(super) const CLIENT_WIDTH: i32 = 640;
pub(super) const CLIENT_HEIGHT: i32 = 320;
pub(super) const BODY_FONT: i32 = 12;
pub(super) const HEADING_FONT: i32 = 24;
pub(super) const CONTROL_LAYOUT: &[(i32, [i32; 4])] = &[
    (101, [58, 174, 430, 20]),
    (102, [44, 134, 240, 18]),
    (100, [516, 166, 80, 34]),
    (103, [44, 248, 544, 18]),
    (IDOK, [458, 272, 80, 34]),
    (IDCANCEL, [548, 272, 80, 34]),
    (104, [368, 272, 80, 34]),
    (105, [458, 272, 80, 34]),
    (106, [44, 134, 550, 88]),
    (107, [44, 210, 550, 18]),
    (108, [44, 281, 400, 19]),
    (109, [44, 134, 550, 116]),
];

pub(super) fn dpi_scale(value: i32, dpi: i32) -> i32 {
    (value * dpi + 48) / 96
}

pub(super) fn control_geometry(
    id: i32,
    mut bounds: [i32; 4],
    maintenance: bool,
    finished: bool,
) -> [i32; 4] {
    if id == IDOK && (maintenance || finished) {
        bounds[0] = 548;
    }
    bounds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_fit_the_client_at_each_monitor_dpi() {
        for dpi in [96, 120, 144, 168, 192, 216, 240, 288, 384] {
            for &(_, [x, y, w, h]) in CONTROL_LAYOUT {
                assert!(dpi_scale(x, dpi) + dpi_scale(w, dpi) <= dpi_scale(CLIENT_WIDTH, dpi));
                assert!(dpi_scale(y, dpi) + dpi_scale(h, dpi) <= dpi_scale(CLIENT_HEIGHT, dpi));
            }
        }
    }
}

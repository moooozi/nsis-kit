//! Window and owner-drawn control rendering. Reads presentation state only.
use super::painting::{BufferedDc, rounded};
use super::*;
pub(super) unsafe fn paint(window: HWND, ui: &Ui) -> LRESULT {
    let mut paint = std::mem::zeroed();
    let target = BeginPaint(window, &mut paint);
    let mut rect = std::mem::zeroed();
    GetClientRect(window, &mut rect);
    let buffer = BufferedDc::new(target, rect);
    let dc = buffer.dc();
    FillRect(dc, &rect, ui.background);
    let product = &ui.branding.name;
    let heading = if let Some(engine) = &ui.engine
        && engine.started()
    {
        if engine.finished.get() {
            if engine.code.get() != 0 {
                "Setup did not complete".into()
            } else if engine.action.get() == "uninstall" {
                format!("{product} uninstalled")
            } else if engine.action.get() == "reinstall" {
                format!("{product} {}", ui.branding.maintenance.action.completed())
            } else {
                format!("{product} installed")
            }
        } else {
            let verb = match engine.action.get() {
                "reinstall" => ui.branding.maintenance.action.progress(),
                "uninstall" => "Uninstalling",
                _ => "Installing",
            };
            format!("{verb} {product}")
        }
    } else if ui.maintenance {
        format!("{product} is already installed")
    } else if !ui.complete {
        format!("Install {product}")
    } else if ui.production && ui.success {
        format!("{product} installed")
    } else if ui.production {
        "Installation failed".into()
    } else if ui.success {
        "Preview complete".into()
    } else {
        "Preview failed".into()
    };
    text(
        dc,
        ui,
        &heading,
        [26, 26, 520, 40],
        ui.heading.get(),
        DT_LEFT,
    );
    text(
        dc,
        ui,
        if ui.engine.as_ref().is_some_and(|engine| engine.started()) {
            ""
        } else if ui.maintenance {
            "Choose what you would like to do with your installation."
        } else {
            &ui.branding.description
        },
        [26, 74, 528, 38],
        ui.normal.get(),
        DT_WORDBREAK,
    );
    paint_logo(dc, ui);
    if let Some(engine) = &ui.engine
        && engine.started()
    {
        let track = RECT {
            left: ui.px(44),
            top: ui.px(230),
            right: ui.px(594),
            bottom: ui.px(240),
        };
        rounded(dc, track, ui.border(), ui.border(), ui.px(10), ui.bg());
        if engine.progress.get() > 0 {
            let fill = RECT {
                right: ui.px(44 + 550 * engine.progress.get() as i32 / 100),
                ..track
            };
            rounded(dc, fill, ACCENT, ACCENT, ui.px(10), ui.bg());
        }
        if engine.active() {
            text(
                dc,
                ui,
                "Please wait while Setup applies your changes.",
                [44, 272, 544, 26],
                ui.normal.get(),
                DT_LEFT,
            );
        }
    } else if !ui.complete && !ui.maintenance {
        rounded(
            dc,
            RECT {
                left: ui.px(44),
                top: ui.px(165),
                right: ui.px(if ui.production { 594 } else { 503 }),
                bottom: ui.px(202),
            },
            if ui.production {
                ui.bg()
            } else {
                ui.surface_color()
            },
            ui.border(),
            ui.px(26),
            ui.bg(),
        );
    } else if ui.complete {
        let message = if ui.production && ui.success {
            &ui.branding.installed_message
        } else if ui.production {
            "Installation did not complete. See the installer error for details."
        } else if ui.success {
            "The stock NSIS engine extracted the harmless text fixture into its temporary directory.\n\nNo files were written to the displayed destination. No installation changes were made.\n\nThe fixture and UI plug-in are cleaned up when Setup exits."
        } else {
            "NSIS could not extract the temporary fixture. No application installation was attempted."
        };
        text(
            dc,
            ui,
            message,
            [44, 136, 544, 116],
            ui.normal.get(),
            DT_WORDBREAK,
        );
    }

    drop(buffer);
    EndPaint(window, &paint);
    0
}
pub(super) unsafe fn button(ui: &Ui, l: LPARAM) -> LRESULT {
    let draw = &*(l as *const DRAWITEMSTRUCT);
    let primary = draw.CtlID == IDOK as u32;
    let buffer = BufferedDc::new(draw.hDC, draw.rcItem);
    let dc = buffer.dc();
    let hovered = ui.hovered.get() == draw.hwndItem;
    let pressed = draw.itemState & ODS_SELECTED != 0;
    let fill = if primary {
        if pressed {
            0xc59d00
        } else if hovered {
            0xf4d865
        } else {
            ACCENT
        }
    } else if pressed {
        if ui.light { 0xc5c5c5 } else { 0x303030 }
    } else if hovered {
        if ui.light { 0xd4d4d4 } else { 0x484848 }
    } else {
        if ui.light { 0xe5e5e5 } else { 0x383838 }
    };
    FillRect(dc, &draw.rcItem, ui.background);
    let focus = draw.itemState & ODS_FOCUS != 0 && draw.itemState & ODS_NOFOCUSRECT == 0;
    rounded(
        dc,
        draw.rcItem,
        fill,
        if focus { ui.fg() } else { fill },
        ui.px(34),
        ui.bg(),
    );
    let mut caption = [0u16; 64];
    GetWindowTextW(draw.hwndItem, caption.as_mut_ptr(), 64);
    let old = SelectObject(dc, ui.normal.get());
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, if primary { 0 } else { ui.fg() });
    let mut rect = draw.rcItem;
    DrawTextW(
        dc,
        caption.as_ptr(),
        -1,
        &mut rect,
        DT_CENTER
            | DT_VCENTER
            | DT_SINGLELINE
            | if draw.itemState & ODS_NOACCEL != 0 {
                DT_HIDEPREFIX
            } else {
                0
            },
    );
    SelectObject(dc, old);
    1
}

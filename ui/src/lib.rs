//! Native NSIS UI with feature-gated installer OS adapters.
//! Payload, registration and integration actions remain in the NSIS scripts.
#![cfg(windows)]
#![allow(unsafe_op_in_unsafe_fn)]

mod abi;
mod branding;
mod layout;
use abi::{Stack, pop, push, wide};
use layout::{CONTROL_LAYOUT, control_geometry, dpi_scale};
mod controls;

mod engine;
#[cfg(feature = "installer")]
mod installer;
#[cfg(feature = "installer")]
mod launch;
#[cfg(feature = "installer")]
mod registry;

#[cfg(feature = "installer")]
mod options;
mod painting;

#[cfg(feature = "installer")]
mod process;

mod render;

#[cfg(feature = "native-svg")]
pub mod svg;
#[cfg(feature = "installer")]
mod win32;

use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    mem::size_of,
    path::Path,
    ptr::*,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::DwmSetWindowAttribute, Gdi::*},
    System::{Com::*, LibraryLoader::GetModuleHandleW},
    UI::{
        Accessibility::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};

const BACKGROUND: u32 = 0x171717;
const SURFACE: u32 = 0x252525;
const BORDER: u32 = 0x404040;
const TEXT: u32 = 0xf5f5f5;
const ACCENT: u32 = 0xe8b900; // COLORREF, not RGB.
const BROWSE: usize = 100;
const PATH_EDIT: usize = 101;
const CLASS: *const u16 = windows_sys::w!("NativeNSISInstaller");

struct Ui {
    branding: branding::Branding,
    dpi: Cell<i32>,
    edit: Cell<HWND>,
    hovered: Cell<HWND>,
    normal: Cell<HFONT>,
    bold: Cell<HFONT>,
    heading: Cell<HFONT>,
    background: HBRUSH,
    surface: HBRUSH,
    icon: Cell<HICON>,
    small_icon: Cell<HICON>,
    large_icon: Cell<HICON>,
    icon_path: String,
    #[cfg(feature = "native-svg")]
    svg: RefCell<Option<svg::SvgLogo>>,
    svg_ready: Cell<bool>,
    complete: bool,
    success: bool,
    accepted: Cell<bool>,
    done: Cell<bool>,
    path: RefCell<String>,
    high_contrast: bool,
    light: bool,
    maintenance: bool,
    production: bool,
    action: Cell<&'static str>,
    engine: Option<engine::Engine>,
}

impl Ui {
    fn px(&self, value: i32) -> i32 {
        dpi_scale(value, self.dpi.get())
    }
    fn bg(&self) -> u32 {
        if self.high_contrast {
            unsafe { GetSysColor(COLOR_WINDOW) }
        } else {
            if self.light { 0xf5f5f5 } else { BACKGROUND }
        }
    }
    fn surface_color(&self) -> u32 {
        if self.high_contrast {
            self.bg()
        } else if self.light {
            0xffffff
        } else {
            SURFACE
        }
    }
    fn border(&self) -> u32 {
        if self.high_contrast {
            self.fg()
        } else if self.light {
            0xd4d4d4
        } else {
            BORDER
        }
    }
    fn fg(&self) -> u32 {
        if self.high_contrast {
            unsafe { GetSysColor(COLOR_WINDOWTEXT) }
        } else {
            if self.light { 0x171717 } else { TEXT }
        }
    }
}

fn light_mode() -> bool {
    use windows_sys::Win32::System::Registry::*;
    // The override is limited to this process, for safe visual fixture testing.
    match std::env::var("NATIVE_INSTALLER_THEME").as_deref() {
        Ok("light") => return true,
        Ok("dark") => return false,
        _ => {}
    }
    unsafe {
        let mut value = 1u32;
        let mut bytes = 4;
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
            wide("AppsUseLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut value as *mut u32).cast(),
            &mut bytes,
        );
        value != 0
    }
}

const WINDOW_STYLE: u32 =
    WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;

unsafe fn window_extent(dpi: i32) -> RECT {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: dpi_scale(layout::CLIENT_WIDTH, dpi),
        bottom: dpi_scale(layout::CLIENT_HEIGHT, dpi),
    };
    AdjustWindowRectExForDpi(&mut rect, WINDOW_STYLE, 0, WS_EX_CONTROLPARENT, dpi as u32);
    rect
}

unsafe fn load_icon(ui: &Ui, pixels: i32) -> HICON {
    if ui.icon_path == "@103" {
        // NSIS already embeds its application icon as RT_GROUP_ICON 103.
        LoadImageW(
            GetModuleHandleW(null()),
            103usize as *const u16,
            IMAGE_ICON,
            pixels,
            pixels,
            0,
        ) as HICON
    } else {
        LoadImageW(
            null_mut(),
            wide(&ui.icon_path).as_ptr(),
            IMAGE_ICON,
            pixels,
            pixels,
            LR_LOADFROMFILE,
        ) as HICON
    }
}

unsafe fn update_logo(window: HWND, ui: &Ui) {
    ui.svg_ready.set(false);
    #[cfg(feature = "native-svg")]
    if std::env::var_os("NATIVE_INSTALLER_FORCE_ICO").is_none() {
        let mut renderer = ui.svg.borrow_mut();
        let result = (|| -> windows::core::Result<()> {
            if renderer.is_none() {
                *renderer = Some(svg::SvgLogo::new(&ui.branding.logo)?);
            }
            let logo = renderer.as_mut().expect("renderer initialized");
            if logo.render(ui.px(48) as u32).is_err() {
                // Rebuild device-dependent resources once; otherwise use ICO.
                *logo = svg::SvgLogo::new(&ui.branding.logo)?;
                logo.render(ui.px(48) as u32)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => ui.svg_ready.set(true),
            Err(error) => {
                *renderer = None;
                windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW(
                    wide(&format!("Installer SVG fallback: {error}")).as_ptr(),
                );
            }
        }
    }
    let renderer = if ui.svg_ready.get() {
        1
    } else if cfg!(feature = "native-svg") {
        3
    } else {
        2
    };
    SetPropW(
        window,
        wide("NativeInstaller.NsisLogoRenderer").as_ptr(),
        renderer as HANDLE,
    );
}

unsafe fn paint_logo(dc: HDC, ui: &Ui) {
    #[cfg(feature = "native-svg")]
    if ui.svg_ready.get()
        && ui
            .svg
            .borrow()
            .as_ref()
            .is_some_and(|logo| unsafe { logo.paint(dc, ui.px(562), ui.px(28)) })
    {
        return;
    }
    DrawIconEx(
        dc,
        ui.px(562),
        ui.px(28),
        ui.icon.get(),
        ui.px(48),
        ui.px(48),
        0,
        null_mut(),
        DI_NORMAL,
    );
}

unsafe fn relayout(window: HWND, ui: &Ui) {
    let finished = ui.complete
        || ui
            .engine
            .as_ref()
            .is_some_and(|engine| engine.finished.get());
    for &(id, bounds) in CONTROL_LAYOUT {
        let [x, y, mut width, height] = control_geometry(id, bounds, ui.maintenance, finished);
        if ui.production && id == PATH_EDIT as i32 {
            width = 522;
        }
        let control = GetDlgItem(window, id);
        if !control.is_null() {
            SetWindowPos(
                control,
                null_mut(),
                ui.px(x),
                ui.px(y),
                ui.px(width),
                ui.px(height),
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOREDRAW,
            );
            let font = if id == 102 {
                ui.bold.get()
            } else {
                ui.normal.get()
            };
            SendMessageW(control, WM_SETFONT, font as usize, 0);
        }
    }
}

unsafe fn update_dpi(window: HWND, ui: &Ui, dpi: i32) {
    let normal = font(dpi_scale(layout::BODY_FONT, dpi), FW_NORMAL);
    let bold = font(dpi_scale(layout::BODY_FONT, dpi), FW_BOLD);
    let heading = font(dpi_scale(layout::HEADING_FONT, dpi), FW_BOLD);
    let previous = [
        ui.normal.replace(normal),
        ui.bold.replace(bold),
        ui.heading.replace(heading),
    ];
    ui.dpi.set(dpi);
    // Reload from original frames, never rescale an icon from the previous monitor.
    let logo = load_icon(ui, ui.px(48));
    let small = load_icon(ui, GetSystemMetricsForDpi(SM_CXSMICON, dpi as u32));
    let large = load_icon(ui, GetSystemMetricsForDpi(SM_CXICON, dpi as u32));
    let old_icons = [
        ui.icon.replace(logo),
        ui.small_icon.replace(small),
        ui.large_icon.replace(large),
    ];
    SendMessageW(window, WM_SETICON, ICON_SMALL as usize, small as isize);
    SendMessageW(window, WM_SETICON, ICON_BIG as usize, large as isize);
    relayout(window, ui);
    engine_controls(window, ui);
    update_logo(window, ui);
    for handle in previous {
        if !handle.is_null() {
            DeleteObject(handle);
        }
    }
    for handle in old_icons {
        if !handle.is_null() {
            DestroyIcon(handle);
        }
    }
}

unsafe fn style_button(window: HWND, ui: &Ui) {
    if !ui.high_contrast {
        SetWindowSubclass(
            window,
            Some(controls::button_proc),
            1,
            ui as *const Ui as usize,
        );
    }
}

unsafe fn font(height: i32, weight: u32) -> HFONT {
    CreateFontW(
        -height,
        0,
        0,
        0,
        weight as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide("Segoe UI").as_ptr(),
    )
}

unsafe fn text(dc: HDC, ui: &Ui, value: &str, bounds: [i32; 4], font: HFONT, flags: u32) {
    let [x, y, w, h] = bounds;
    let previous = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    SetTextColor(dc, ui.fg());
    let mut rect = RECT {
        left: ui.px(x),
        top: ui.px(y),
        right: ui.px(x + w),
        bottom: ui.px(y + h),
    };
    let content = wide(value);
    DrawTextW(dc, content.as_ptr(), -1, &mut rect, flags | DT_NOPREFIX);
    SelectObject(dc, previous);
}

unsafe fn child(
    window: HWND,
    ui: &Ui,
    class: &str,
    caption: &str,
    id: usize,
    bounds: [i32; 4],
    style: u32,
) -> HWND {
    let handle = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(caption).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        ui.px(bounds[0]),
        ui.px(bounds[1]),
        ui.px(bounds[2]),
        ui.px(bounds[3]),
        window,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(handle, WM_SETFONT, ui.normal.get() as usize, 1);
    handle
}

unsafe extern "system" fn window_proc(window: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(l as *const CREATESTRUCTW);
        SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as _);
    }
    let state = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut Ui;
    if state.is_null() {
        return DefWindowProcW(window, msg, w, l);
    }
    // Avoid holding a Rust mutable reference across reentrant Win32 calls.
    let ui = &*state;
    match msg {
        WM_CREATE => {
            update_dpi(window, ui, GetDpiForWindow(window) as i32);
            let button_style = WS_TABSTOP
                | if ui.high_contrast {
                    BS_PUSHBUTTON as u32
                } else {
                    BS_OWNERDRAW as u32
                };
            if !ui.complete && !ui.maintenance {
                let edit = child(
                    window,
                    ui,
                    "EDIT",
                    &ui.path.borrow(),
                    PATH_EDIT,
                    [58, 174, if ui.production { 522 } else { 430 }, 20],
                    ES_AUTOHSCROLL as u32
                        | if ui.production {
                            WS_DISABLED
                        } else {
                            WS_TABSTOP
                        },
                );
                SendMessageW(edit, EM_SETLIMITTEXT, 1023, 0);
                if ui.production {
                    SendMessageW(edit, EM_SETREADONLY, 1, 0);
                }
                ui.edit.set(edit);
                child(
                    window,
                    ui,
                    "STATIC",
                    "Installation folder",
                    102,
                    [44, 134, 240, 18],
                    0,
                );
                if !ui.production {
                    let browse = child(
                        window,
                        ui,
                        "BUTTON",
                        "&Browse...",
                        BROWSE,
                        [516, 166, 80, 34],
                        button_style,
                    );
                    style_button(browse, ui);
                }
                child(window, ui, "STATIC", "", 103, [44, 248, 544, 18], 0);
                let scope = if ui.production {
                    "Installs for all users."
                } else {
                    "Preview only; no application is installed."
                };
                child(window, ui, "STATIC", scope, 107, [44, 210, 550, 18], 0);
                let space = if ui.production {
                    branding::required_space_text(ui.branding.required_kib)
                } else {
                    "Space required: preview only".into()
                };
                child(window, ui, "STATIC", &space, 108, [44, 281, 400, 19], 0);
            }
            let install = child(
                window,
                ui,
                "BUTTON",
                if ui.complete {
                    "Close"
                } else if ui.maintenance {
                    "&Launch"
                } else {
                    "&Install"
                },
                IDOK as usize,
                [458, 272, 80, 34],
                button_style,
            );
            style_button(install, ui);
            if !ui.complete {
                let cancel = child(
                    window,
                    ui,
                    "BUTTON",
                    if ui.maintenance {
                        ui.branding.maintenance.action.label()
                    } else {
                        "Cancel"
                    },
                    if ui.maintenance {
                        105
                    } else {
                        IDCANCEL as usize
                    },
                    [548, 272, 80, 34],
                    button_style,
                );
                style_button(cancel, ui);
            }
            if ui.maintenance {
                let uninstall = child(
                    window,
                    ui,
                    "BUTTON",
                    "&Uninstall",
                    104,
                    [368, 272, 80, 34],
                    button_style,
                );
                style_button(uninstall, ui);
            }
            if ui.maintenance {
                child(
                    window,
                    ui,
                    "STATIC",
                    &ui.branding.maintenance.description(&ui.branding.name),
                    109,
                    [44, 134, 550, 116],
                    0,
                );
            }
            if ui.engine.is_some() {
                let status = child(window, ui, "STATIC", "", 106, [44, 134, 550, 88], 0);
                ShowWindow(status, SW_HIDE);
            }
            relayout(window, ui);
            0
        }
        WM_GETDPISCALEDSIZE => {
            let rect = window_extent(w as i32);
            let size = &mut *(l as *mut SIZE);
            size.cx = rect.right - rect.left;
            size.cy = rect.bottom - rect.top;
            1
        }
        WM_DPICHANGED => {
            let suggested = *(l as *const RECT);
            update_dpi(window, ui, (w & 0xffff) as i32);
            SetWindowPos(
                window,
                null_mut(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOREDRAW,
            );
            RedrawWindow(
                window,
                null(),
                null_mut(),
                RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_NOERASE,
            );
            0
        }
        WM_PAINT => render::paint(window, ui),
        WM_DRAWITEM => render::button(ui, l),
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => {
            let dc = w as HDC;
            let edit = l as HWND == ui.edit.get();
            let fixed_path = edit && ui.production;
            SetTextColor(
                dc,
                if fixed_path {
                    if ui.high_contrast {
                        GetSysColor(COLOR_GRAYTEXT)
                    } else if ui.light {
                        0x777777
                    } else {
                        0x999999
                    }
                } else {
                    ui.fg()
                },
            );
            SetBkColor(
                dc,
                if edit && !fixed_path && !ui.high_contrast {
                    ui.surface_color()
                } else {
                    ui.bg()
                },
            );
            if edit && !fixed_path {
                ui.surface as isize
            } else {
                ui.background as isize
            }
        }
        WM_COMMAND => {
            if let Some(engine) = &ui.engine {
                if engine.active() {
                    return 0;
                }
                if engine.finished.get() {
                    if w & 0xffff == IDOK as usize || w & 0xffff == IDCANCEL as usize {
                        DestroyWindow(window);
                    }
                    return 0;
                }
            }
            match w & 0xffff {
                BROWSE => controls::browse(window, ui),
                id if ui.maintenance && (id == IDOK as usize || id == 105 || id == 104) => {
                    ui.action.set(if id == IDOK as usize {
                        "launch"
                    } else if id == 105 {
                        "reinstall"
                    } else {
                        "uninstall"
                    });
                    if let Some(engine) = &ui.engine {
                        if ui.action.get() == "launch" {
                            #[cfg(feature = "installer")]
                            let result = installer::launch_frontend(&ui.path.borrow());
                            #[cfg(not(feature = "installer"))]
                            let result: Result<(), String> =
                                Err("Preview builds cannot launch installed applications.".into());
                            match result {
                                Ok(_) => {
                                    engine.code.set(0);
                                    DestroyWindow(window);
                                }
                                Err(error) => {
                                    *engine.status.borrow_mut() = error;
                                    engine_controls(window, ui);
                                }
                            }
                        } else {
                            engine.start(window, ui.action.get(), &ui.path.borrow(), ui);
                            engine_controls(window, ui);
                        }
                    } else {
                        ui.accepted.set(true);
                        DestroyWindow(window);
                    }
                }
                id if id == IDOK as usize => {
                    if !ui.complete {
                        let mut value = [0u16; 1024];
                        GetWindowTextW(ui.edit.get(), value.as_mut_ptr(), value.len() as i32);
                        let end = value.iter().position(|c| *c == 0).unwrap_or(value.len());
                        let path = String::from_utf16_lossy(&value[..end]).trim().to_owned();

                        if !Path::new(&path).is_absolute() {
                            SetWindowTextW(
                                GetDlgItem(window, 103),
                                wide("Enter an absolute installation folder path.").as_ptr(),
                            );
                            SetFocus(ui.edit.get());
                            return 0;
                        }
                        *ui.path.borrow_mut() = path;
                        if let Some(engine) = &ui.engine {
                            engine.start(window, "install", &ui.path.borrow(), ui);
                            engine_controls(window, ui);
                            return 0;
                        }
                        ui.accepted.set(true);
                    }
                    DestroyWindow(window);
                }
                id if id == IDCANCEL as usize => {
                    DestroyWindow(window);
                }
                _ => {}
            }
            0
        }
        WM_TIMER if w == 1 => {
            if ui.engine.as_ref().is_some_and(|engine| engine.poll(window)) {
                engine_controls(window, ui);
            }
            0
        }
        WM_COPYDATA => {
            if l != 0
                && ui.engine.as_ref().is_some_and(|engine| {
                    engine.receive(
                        w,
                        &*(l as *const windows_sys::Win32::System::DataExchange::COPYDATASTRUCT),
                    )
                })
            {
                if let Some(engine) = &ui.engine {
                    engine.finish_foreground_handoff(window);
                }
                engine_controls(window, ui);
                1
            } else {
                0
            }
        }
        WM_CLOSE => {
            if ui.engine.as_ref().is_some_and(|engine| engine.active()) {
                return 0;
            }
            DestroyWindow(window);
            0
        }
        WM_DESTROY => {
            RemovePropW(window, wide("NativeInstaller.NsisLogoRenderer").as_ptr());
            RemovePropW(window, wide("NativeInstaller.NsisOperation").as_ptr());
            RemovePropW(window, wide("NativeInstaller.NsisWorkerPID").as_ptr());
            RemovePropW(window, wide("NativeInstaller.NsisFocusBeforePID").as_ptr());
            RemovePropW(window, wide("NativeInstaller.NsisFocusGrant").as_ptr());
            RemovePropW(window, wide("NativeInstaller.NsisFocusHandoffs").as_ptr());
            RemovePropW(
                window,
                wide("NativeInstaller.NsisFocusFrontendResult").as_ptr(),
            );
            RemovePropW(window, wide("NativeInstaller.NsisFocusResult").as_ptr());
            ui.done.set(true);
            0
        }
        WM_ERASEBKGND => 1,
        _ => DefWindowProcW(window, msg, w, l),
    }
}

unsafe fn engine_controls(window: HWND, ui: &Ui) {
    let Some(engine) = &ui.engine else {
        return;
    };
    let started = engine.started();
    for &(id, _) in CONTROL_LAYOUT {
        let control = GetDlgItem(window, id);
        if !control.is_null() {
            ShowWindow(
                control,
                if !started || (id == IDOK && engine.finished.get()) {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
    }
    if engine.finished.get() {
        relayout(window, ui);
        SetWindowTextW(GetDlgItem(window, IDOK), wide("Close").as_ptr());
        SetFocus(GetDlgItem(window, IDOK));
    }
    let status = GetDlgItem(window, 106);
    let status_text = engine.status.borrow().clone();
    if !status.is_null() {
        SetWindowTextW(status, wide(&status_text).as_ptr());
        SetWindowPos(
            status,
            null_mut(),
            ui.px(44),
            ui.px(if started || ui.maintenance { 134 } else { 238 }),
            ui.px(550),
            ui.px(if started { 88 } else { 28 }),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        ShowWindow(
            status,
            if status_text.is_empty() {
                SW_HIDE
            } else {
                SW_SHOW
            },
        );
    }
    SetPropW(
        window,
        wide("NativeInstaller.NsisOperation").as_ptr(),
        (if engine.active() {
            1
        } else if engine.finished.get() {
            2
        } else {
            0
        }) as HANDLE,
    );
    EnableMenuItem(
        GetSystemMenu(window, 0),
        SC_CLOSE,
        MF_BYCOMMAND
            | if engine.active() {
                MF_GRAYED
            } else {
                MF_ENABLED
            },
    );
    InvalidateRect(window, null(), 0);
}

unsafe fn show(
    path: String,
    icon: &str,
    complete: bool,
    success: bool,
    maintenance: bool,
    production: bool,
    engine: Option<engine::Engine>,
) -> Result<Option<String>, u32> {
    let previous_dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let com = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
    let instance = GetModuleHandleW(null());
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: CLASS,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    RegisterClassW(&class);
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..std::mem::zeroed()
    };
    SystemParametersInfoW(
        SPI_GETHIGHCONTRAST,
        contrast.cbSize,
        (&mut contrast as *mut HIGHCONTRASTW).cast(),
        0,
    );
    let high_contrast = contrast.dwFlags & HCF_HIGHCONTRASTON != 0;
    let light = light_mode();
    let dpi = GetDpiForSystem() as i32;
    let mut ui = Box::new(Ui {
        branding: branding::current(),
        dpi: Cell::new(dpi),
        edit: Cell::new(null_mut()),
        hovered: Cell::new(null_mut()),
        normal: Cell::new(null_mut()),
        bold: Cell::new(null_mut()),
        heading: Cell::new(null_mut()),
        background: CreateSolidBrush(if high_contrast {
            GetSysColor(COLOR_WINDOW)
        } else {
            if light { 0xf5f5f5 } else { BACKGROUND }
        }),
        surface: CreateSolidBrush(if high_contrast {
            GetSysColor(COLOR_WINDOW)
        } else {
            if light { 0xffffff } else { SURFACE }
        }),
        icon: Cell::new(null_mut()),
        small_icon: Cell::new(null_mut()),
        large_icon: Cell::new(null_mut()),
        icon_path: icon.to_owned(),
        #[cfg(feature = "native-svg")]
        svg: RefCell::new(None),
        svg_ready: Cell::new(false),
        complete,
        success,
        accepted: Cell::new(false),
        done: Cell::new(false),
        path: RefCell::new(path),
        high_contrast,
        light,
        maintenance,
        production,
        action: Cell::new("cancel"),
        engine,
    });
    let style = WINDOW_STYLE;
    let rect = window_extent(dpi);
    let mut cursor = std::mem::zeroed();
    GetCursorPos(&mut cursor);
    let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    let mut monitor_info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..std::mem::zeroed()
    };
    GetMonitorInfoW(monitor, &mut monitor_info);
    let work = monitor_info.rcWork;
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let window = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        CLASS,
        wide(&format!(
            "Setup - {}{}",
            ui.branding.name,
            if production { "" } else { " (NSIS preview)" }
        ))
        .as_ptr(),
        style,
        work.left + (work.right - work.left - width) / 2,
        work.top + (work.bottom - work.top - height) / 2,
        width,
        height,
        null_mut(),
        null_mut(),
        instance,
        (&mut *ui as *mut Ui).cast(),
    );
    let creation_error = GetLastError();
    if !window.is_null() {
        if !high_contrast {
            let dark = i32::from(!light);
            DwmSetWindowAttribute(window, 20, (&dark as *const i32).cast(), 4);
            DwmSetWindowAttribute(window, 35, (&ui.bg() as *const u32).cast(), 4);
        }
        let rect = window_extent(ui.dpi.get());
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        SetWindowPos(
            window,
            null_mut(),
            work.left + (work.right - work.left - width) / 2,
            work.top + (work.bottom - work.top - height) / 2,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SendMessageW(
            window,
            WM_CHANGEUISTATE,
            (((UISF_HIDEACCEL | UISF_HIDEFOCUS) as usize) << 16) | UIS_SET as usize,
            0,
        );
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
        SetFocus(GetDlgItem(window, IDOK));
        let mut message = std::mem::zeroed();
        while !ui.done.get() {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result <= 0 {
                if result == 0 {
                    PostQuitMessage(message.wParam as i32);
                }
                DestroyWindow(window);
                break;
            }
            if IsDialogMessageW(window, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    UnregisterClassW(CLASS, instance);
    DeleteObject(ui.normal.get());
    DeleteObject(ui.bold.get());
    DeleteObject(ui.heading.get());
    DeleteObject(ui.background);
    DeleteObject(ui.surface);
    for icon in [ui.icon.get(), ui.small_icon.get(), ui.large_icon.get()] {
        if !icon.is_null() {
            DestroyIcon(icon);
        }
    }
    #[cfg(feature = "native-svg")]
    ui.svg.borrow_mut().take();
    if com >= 0 {
        CoUninitialize();
    }
    if !previous_dpi.is_null() {
        SetThreadDpiAwarenessContext(previous_dpi);
    }
    if window.is_null() {
        Err(creation_error)
    } else if let Some(engine) = &ui.engine {
        Ok(Some(engine.code.get().to_string()))
    } else if ui.accepted.get() {
        Ok(Some(if maintenance {
            ui.action.get().to_owned()
        } else {
            ui.path.borrow().clone()
        }))
    } else {
        Ok(None)
    }
}

/// NSIS plug-in entry points use cdecl, including on x86.
///
/// # Safety
/// Called by NSIS with its Unicode stack ABI and valid string capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Choose(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let path = pop(stack, size as usize);
    let icon = pop(stack, size as usize);
    let (status, selected) = match show(path, &icon, false, true, false, false, None) {
        Ok(Some(path)) => ("install".to_owned(), path),
        Ok(None) => ("cancel".to_owned(), String::new()),
        Err(code) => (
            format!("Window creation failed: Win32 error {code}"),
            String::new(),
        ),
    };
    push(stack, size as usize, &selected);
    push(stack, size as usize, &status);
}

/// # Safety
/// Called by NSIS with its Unicode stack ABI and valid string capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Complete(
    _: HWND,
    size: i32,
    _: *mut u16,
    stack: *mut *mut Stack,
    _: *mut c_void,
) {
    if size <= 0 {
        return;
    }
    let status = pop(stack, size as usize);
    let icon = pop(stack, size as usize);
    let _ = show(
        String::new(),
        &icon,
        true,
        status == "success",
        false,
        false,
        None,
    );
}

/// Production selection adapter. Returns action followed by destination on the NSIS stack.
/// Mode is `install` or `maintenance`; no system changes are made here.
///
/// # Safety
/// Called by NSIS with its Unicode stack ABI and valid string capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Select(
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
    let path = pop(stack, size as usize);
    let maintenance = mode == "maintenance" || mode == "maintenance-preview";
    let (status, selected) = match show(
        path.clone(),
        "@103",
        false,
        true,
        maintenance,
        mode != "maintenance-preview",
        None,
    ) {
        Ok(Some(result)) if maintenance => (result, path),
        Ok(Some(result)) => ("install".into(), result),
        Ok(None) => ("cancel".into(), String::new()),
        Err(code) => (format!("UI failed: {code}"), String::new()),
    };
    push(stack, size as usize, &selected);
    push(stack, size as usize, &status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_primary_uses_the_rightmost_footer_slot() {
        let install = [458, 272, 80, 34];
        let rightmost = [548, 272, 80, 34];
        assert_eq!(control_geometry(IDOK, install, false, false), install);
        for maintenance in [false, true] {
            assert_eq!(
                control_geometry(IDOK, install, maintenance, true),
                rightmost
            );
            for dpi in [96, 120, 144, 168, 192, 216, 240, 288, 384] {
                let [x, y, width, height] = control_geometry(IDOK, install, maintenance, true);
                assert_eq!(dpi_scale(x, dpi), dpi_scale(rightmost[0], dpi));
                assert_eq!(dpi_scale(y + height, dpi), dpi_scale(306, dpi));
                assert!(
                    dpi_scale(x, dpi) + dpi_scale(width, dpi)
                        < dpi_scale(layout::CLIENT_WIDTH, dpi)
                );
            }
        }
    }

    #[test]
    fn layout_uses_base_coordinates_at_every_dpi() {
        for dpi in [96, 120, 144, 168, 192, 216, 240, 288, 384] {
            for &(_, [x, y, width, height]) in CONTROL_LAYOUT {
                assert!(
                    dpi_scale(x, dpi) + dpi_scale(width, dpi)
                        <= dpi_scale(layout::CLIENT_WIDTH, dpi)
                );
                assert!(
                    dpi_scale(y, dpi) + dpi_scale(height, dpi)
                        <= dpi_scale(layout::CLIENT_HEIGHT, dpi)
                );
            }
        }
        assert_eq!(
            [96, 120, 144, 168, 192, 216].map(|dpi| dpi_scale(48, dpi)),
            [48, 60, 72, 84, 96, 108]
        );
        // A return to 100% is computed from DIPs, not a rounded previous size.
        assert_eq!(dpi_scale(640, 96), 640);
    }

    #[test]
    fn nsis_stack_preserves_unicode_and_lifo_order() {
        unsafe {
            let mut stack = null_mut();
            push(&mut stack, 1024, r"C:\é¢„è§ˆ\ExampleApp");
            push(&mut stack, 1024, "install");
            assert_eq!(pop(&mut stack, 1024), "install");
            assert_eq!(pop(&mut stack, 1024), r"C:\é¢„è§ˆ\ExampleApp");
            assert!(stack.is_null());
            assert_eq!(pop(&mut stack, 1024), "");
        }
    }

    #[test]
    fn nsis_stack_respects_capacity_and_null_termination() {
        unsafe {
            let mut stack = null_mut();
            push(&mut stack, 4, "abcdef");
            assert_eq!(pop(&mut stack, 4), "abc");
            push(&mut stack, 1, "abcdef");
            assert_eq!(pop(&mut stack, 1), "");
            push(&mut stack, 0, "ignored");
            assert!(stack.is_null());
        }
    }
}

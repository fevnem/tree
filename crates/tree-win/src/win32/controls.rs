//! Stock controls: buttons, labels, edits, progress bar, status bar.

use super::wide;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::HFONT;
use windows::Win32::UI::Controls::{PBM_SETPOS, PBM_SETRANGE32, PBM_SETSTATE, PBST_ERROR, PBST_NORMAL, PBST_PAUSED};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SendMessageW, ES_AUTOHSCROLL, ES_MULTILINE, ES_AUTOVSCROLL,
    WS_BORDER, WS_CHILD, WS_EX_CLIENTEDGE, WS_TABSTOP, WS_VISIBLE,
};

pub const ID_ADD: i32 = 1001;
pub const ID_PAUSE: i32 = 1002;
pub const ID_RESUME: i32 = 1003;
pub const ID_REMOVE: i32 = 1004;
pub const ID_FOLDER: i32 = 1005;
pub const ID_FILTER: i32 = 1006;
pub const ID_LIST: i32 = 1007;
pub const ID_PROGRESS: i32 = 1008;
pub const ID_STATUS: i32 = 1009;
pub const ID_URL: i32 = 1010;
pub const ID_DIR: i32 = 1011;
pub const ID_BROWSE: i32 = 1012;
pub const ID_OK: i32 = 1013;
pub const ID_CANCEL: i32 = 1014;
pub const ID_LABEL_URL: i32 = 1015;
pub const ID_LABEL_DIR: i32 = 1016;

#[allow(clippy::too_many_arguments)]
fn child(
    class: PCWSTR,
    text: PCWSTR,
    style: u32,
    ex: u32,
    parent: HWND,
    id: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    unsafe {
        CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(ex),
            class,
            text,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style),
            x,
            y,
            w,
            h,
            Some(parent),
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(id as usize as *mut core::ffi::c_void)),
            Some(super::window::instance()),
            None,
        )
        .unwrap_or_default()
    }
}

pub fn button(parent: HWND, id: i32, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let t = wide::wide(text);
    child(
        w!("BUTTON"),
        wide::pcw(&t),
        WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | windows::Win32::UI::WindowsAndMessaging::BS_PUSHBUTTON as u32,
        0,
        parent,
        id,
        x,
        y,
        w,
        h,
    )
}

pub fn default_button(parent: HWND, id: i32, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let t = wide::wide(text);
    child(
        w!("BUTTON"),
        wide::pcw(&t),
        WS_CHILD.0
            | WS_VISIBLE.0
            | WS_TABSTOP.0
            | windows::Win32::UI::WindowsAndMessaging::BS_DEFPUSHBUTTON as u32,
        0,
        parent,
        id,
        x,
        y,
        w,
        h,
    )
}

pub fn label(parent: HWND, id: i32, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let t = wide::wide(text);
    child(
        w!("STATIC"),
        wide::pcw(&t),
        WS_CHILD.0 | WS_VISIBLE.0,
        0,
        parent,
        id,
        x,
        y,
        w,
        h,
    )
}

pub fn edit(parent: HWND, id: i32, multiline: bool, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let mut style = WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0 | WS_BORDER.0 | ES_AUTOHSCROLL as u32;
    if multiline {
        style |= ES_MULTILINE as u32
            | ES_AUTOVSCROLL as u32
            | windows::Win32::UI::WindowsAndMessaging::ES_WANTRETURN as u32;
    }
    child(
        w!("EDIT"),
        PCWSTR::null(),
        style,
        WS_EX_CLIENTEDGE.0,
        parent,
        id,
        x,
        y,
        w,
        h,
    )
}

pub fn progress(parent: HWND, id: i32, x: i32, y: i32, w: i32, h: i32) -> HWND {
    child(
        w!("msctls_progress32"),
        PCWSTR::null(),
        WS_CHILD.0 | WS_VISIBLE.0,
        0,
        parent,
        id,
        x,
        y,
        w,
        h,
    )
}

pub fn statusbar(parent: HWND, id: i32) -> HWND {
    child(
        windows::Win32::UI::Controls::STATUSCLASSNAMEW,
        PCWSTR::null(),
        (WS_CHILD | WS_VISIBLE).0,
        0,
        parent,
        id,
        0,
        0,
        0,
        0,
    )
}

pub fn set_font(hwnd: HWND, font: HFONT) {
    unsafe {
        SendMessageW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

/// 0..=1000 so the bar moves smoothly instead of in whole percents.
pub fn progress_set(bar: HWND, permille: u16, paused: bool, error: bool) {
    unsafe {
        SendMessageW(bar, PBM_SETPOS, Some(WPARAM(permille as usize)), Some(LPARAM(0)));
        let state = if error {
            PBST_ERROR
        } else if paused {
            PBST_PAUSED
        } else {
            PBST_NORMAL
        };
        SendMessageW(bar, PBM_SETSTATE, Some(WPARAM(state as usize)), Some(LPARAM(0)));
    }
}

pub fn progress_range(bar: HWND, max: i32) {
    unsafe {
        SendMessageW(bar, PBM_SETRANGE32, Some(WPARAM(0)), Some(LPARAM(max as isize)));
    }
}

pub fn status_parts(sb: HWND, widths: &[i32]) {
    unsafe {
        SendMessageW(
            sb,
            windows::Win32::UI::Controls::SB_SETPARTS,
            Some(WPARAM(widths.len())),
            Some(LPARAM(widths.as_ptr() as isize)),
        );
    }
}

pub fn status_text(sb: HWND, part: usize, s: &str) {
    let t = wide::wide(s);
    unsafe {
        SendMessageW(
            sb,
            windows::Win32::UI::Controls::SB_SETTEXTW,
            Some(WPARAM(part)),
            Some(LPARAM(wide::pcw(&t).0 as isize)),
        );
    }
}

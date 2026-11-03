//! Stock controls: buttons, labels and edits.

use super::wide;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::HFONT;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SendMessageW, ES_AUTOHSCROLL, ES_MULTILINE, ES_AUTOVSCROLL,
    WS_BORDER, WS_CHILD, WS_EX_CLIENTEDGE, WS_TABSTOP, WS_VISIBLE,
};

pub const ID_FILTER: i32 = 1006;
pub const ID_LIST: i32 = 1007;
pub const ID_URL: i32 = 1010;
pub const ID_DIR: i32 = 1011;
pub const ID_BROWSE: i32 = 1012;
pub const ID_OK: i32 = 1013;
pub const ID_CANCEL: i32 = 1014;
pub const ID_LABEL_URL: i32 = 1015;
pub const ID_LABEL_DIR: i32 = 1016;

/// A stock push button — used by the Add dialog, where native is the point.
pub fn button(parent: HWND, id: i32, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    let t = wide::wide(text);
    unsafe {
        CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
            w!("BUTTON"),
            wide::pcw(&t),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                WS_CHILD.0 | WS_VISIBLE.0 | WS_TABSTOP.0
                    | windows::Win32::UI::WindowsAndMessaging::BS_PUSHBUTTON as u32,
            ),
            x,
            y,
            w,
            h,
            Some(parent),
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(
                id as usize as *mut core::ffi::c_void,
            )),
            Some(super::window::instance()),
            None,
        )
        .unwrap_or_default()
    }
}

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








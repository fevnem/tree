//! Stock controls: buttons, labels and edits.

use super::wide;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::HFONT;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SendMessageW, ES_AUTOHSCROLL, ES_MULTILINE, ES_AUTOVSCROLL,
    WS_BORDER, WS_CHILD, WS_EX_CLIENTEDGE, WS_TABSTOP, WS_VISIBLE, BS_OWNERDRAW,
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

/// Switch a button to owner-draw so a dark palette applies: stock buttons ignore
/// `SetWindowTheme`, and a white button on a dark dialog reads as broken.
pub fn set_owner_draw(button: HWND, enabled: bool) {
    unsafe {
        let style = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            button,
            windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
        );
        let base = (style as u32) & !(windows::Win32::UI::WindowsAndMessaging::BS_TYPEMASK as u32);
        let style = if enabled {
            base | BS_OWNERDRAW as u32
        } else {
            base | windows::Win32::UI::WindowsAndMessaging::BS_PUSHBUTTON as u32
        };
        windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            button,
            windows::Win32::UI::WindowsAndMessaging::GWL_STYLE,
            style as isize,
        );
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(button), None, true);
    }
}

/// Paint an owner-drawn dialog button in the palette.
pub fn paint_owner_button(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    rect: &windows::Win32::Foundation::RECT,
    text: &str,
    pressed: bool,
    primary: bool,
    colors: crate::win32::theme::Colors,
) {
    use windows::Win32::Graphics::Gdi::{
        CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, RoundRect, SelectObject,
        SetBkMode, SetTextColor, DRAW_TEXT_FORMAT, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
        HGDIOBJ, PS_NULL, TRANSPARENT,
    };
    let fill = if primary {
        if pressed {
            colors.accent
        } else {
            colors.accent
        }
    } else if pressed {
        crate::win32::theme::lighten(colors.bg, colors.text, 0.18)
    } else {
        crate::win32::theme::lighten(colors.bg, colors.text, 0.10)
    };
    let text_color = if primary { 0x00FF_FFFF } else { colors.text };
    unsafe {
        let brush = CreateSolidBrush(COLORREF(fill));
        let pen = CreatePen(PS_NULL, 0, COLORREF(0));
        let old_b = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_p = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 8, 8);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));

        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(text_color));
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let mut rc = *rect;
        let _ = DrawTextW(
            hdc,
            &mut wide,
            &mut rc,
            DRAW_TEXT_FORMAT(DT_CENTER.0 | DT_SINGLELINE.0 | DT_VCENTER.0),
        );
        let _ = FillRect;
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








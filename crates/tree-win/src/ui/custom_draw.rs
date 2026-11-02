//! Custom painting for dark palettes.
//!
//! Windows only themes the chrome it wants to theme: buttons, the ListView
//! header and the status bar ignore `SetWindowTheme` on many builds. Rather than
//! ship a half-dark window, those surfaces are painted from the palette when it
//! is dark; light mode keeps the stock controls untouched.

use crate::app::App;
use crate::win32::wide;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, FrameRect, SelectObject, SetBkMode,
    SetTextColor, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER, HDC, HGDIOBJ,
    TRANSPARENT,
};
use windows::Win32::UI::Controls::{
    HDITEMW, HDM_GETITEMW, HDI_TEXT, NMCUSTOMDRAW, NMLVCUSTOMDRAW, CDDS_ITEMPREPAINT, CDDS_PREPAINT,
    CDDS_SUBITEM, CDRF_DODEFAULT, CDRF_NEWFONT, CDRF_NOTIFYITEMDRAW, CDRF_NOTIFYSUBITEMDRAW,
    CDRF_SKIPDEFAULT, ODS_DISABLED, ODS_SELECTED,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowTextLengthW, GetWindowTextW, SendMessageW,
};

fn darker(rgb: u32, amount: u32) -> u32 {
    let r = (rgb & 0xFF).saturating_sub(amount);
    let g = ((rgb >> 8) & 0xFF).saturating_sub(amount);
    let b = ((rgb >> 16) & 0xFF).saturating_sub(amount);
    r | (g << 8) | (b << 16)
}

fn fill(hdc: HDC, rect: &RECT, rgb: u32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(rgb));
        FillRect(hdc, rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn frame(hdc: HDC, rect: &RECT, rgb: u32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(rgb));
        FrameRect(hdc, rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn text(hdc: HDC, rect: &mut RECT, s: &str, color: u32, format: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT) {
    // No trailing NUL: DrawTextW takes a counted slice, and the NUL would be
    // painted as a missing-glyph box.
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(color));
        DrawTextW(hdc, &mut buf, rect, format);
    }
}

fn header_item_text(header: HWND, index: usize) -> String {
    let mut buf = [0u16; 256];
    let mut item = HDITEMW {
        mask: HDI_TEXT,
        pszText: windows::core::PWSTR(buf.as_mut_ptr()),
        cchTextMax: buf.len() as i32,
        ..Default::default()
    };
    unsafe {
        SendMessageW(
            header,
            HDM_GETITEMW,
            Some(WPARAM(index)),
            Some(LPARAM(&mut item as *mut HDITEMW as isize)),
        );
    }
    wide::from_wide(&buf)
}

/// Column headers.
pub fn header(app: &App, draw: &NMCUSTOMDRAW) -> Option<isize> {
    if !app.palette.colors.dark {
        return None;
    }
    let colors = app.palette.colors;
    match draw.dwDrawStage.0 {
        s if s == CDDS_PREPAINT.0 => Some(CDRF_NOTIFYITEMDRAW as isize),
        s if s == CDDS_ITEMPREPAINT.0 => {
            let hdc = draw.hdc;
            let rect = draw.rc;
            fill(hdc, &rect, colors.bg);
            let sep = RECT {
                left: rect.right - 1,
                ..rect
            };
            fill(hdc, &sep, darker(colors.bg, 14));

            let mut inner = rect;
            inner.left += 8;
            inner.right -= 4;
            let old = unsafe { SelectObject(hdc, HGDIOBJ(app.font.0)) };
            text(
                hdc,
                &mut inner,
                &header_item_text(draw.hdr.hwndFrom, draw.dwItemSpec),
                colors.text,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            unsafe {
                SelectObject(hdc, old);
            }
            Some(CDRF_SKIPDEFAULT as isize)
        }
        _ => None,
    }
}

/// Rows: colours per sub-item; selection keeps the system highlight.
pub fn list(app: &App, draw: &mut NMLVCUSTOMDRAW) -> Option<isize> {
    let colors = app.palette.colors;
    if !colors.dark {
        return None;
    }
    let stage = draw.nmcd.dwDrawStage.0;
    match stage {
        s if s == CDDS_PREPAINT.0 => Some(CDRF_NOTIFYITEMDRAW as isize),
        s if s == CDDS_ITEMPREPAINT.0 => Some(CDRF_NOTIFYSUBITEMDRAW as isize),
        s if s == (CDDS_ITEMPREPAINT.0 | CDDS_SUBITEM.0) => {
            let selected = draw.nmcd.uItemState.0 & 0x0001 != 0; // CDIS_SELECTED
            if !selected {
                draw.clrText = COLORREF(colors.text);
                draw.clrTextBk = COLORREF(colors.surface);
            }
            Some(CDRF_NEWFONT as isize)
        }
        _ => Some(CDRF_DODEFAULT as isize),
    }
}

/// Owner-drawn buttons, used only in dark palettes.
pub fn button(app: &App, hwnd_item: HWND, hdc: HDC, rect: &RECT, state: u32) {
    let colors = app.palette.colors;
    let pressed = state & ODS_SELECTED.0 != 0;
    let disabled = state & ODS_DISABLED.0 != 0;

    let bg = if pressed {
        darker(colors.surface, 12)
    } else {
        colors.surface
    };
    let fg = if disabled { colors.dim } else { colors.text };

    fill(hdc, rect, bg);
    frame(hdc, rect, darker(colors.bg, 20));

    let len = unsafe { GetWindowTextLengthW(hwnd_item) };
    if len > 0 {
        let mut buf = vec![0u16; len as usize + 1];
        unsafe {
            GetWindowTextW(hwnd_item, &mut buf);
        }
        let mut inner = *rect;
        inner.top += 1;
        inner.bottom -= 1;
        let old = unsafe { SelectObject(hdc, HGDIOBJ(app.font.0)) };
        text(
            hdc,
            &mut inner,
            &wide::from_wide(&buf),
            fg,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        unsafe {
            SelectObject(hdc, old);
        }
    }
}

/// One status-bar part, painted from the buffer `App` keeps for that part.
pub fn status_part(app: &App, s: &str, hdc: HDC, rect: &RECT) {
    let colors = app.palette.colors;
    fill(hdc, rect, colors.bg);
    let sep = RECT {
        left: rect.right - 1,
        ..*rect
    };
    fill(hdc, &sep, darker(colors.bg, 10));
    let mut inner = *rect;
    inner.left += 8;
    inner.right -= 6;
    let old = unsafe { SelectObject(hdc, HGDIOBJ(app.font.0)) };
    text(
        hdc,
        &mut inner,
        s,
        colors.text,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    unsafe {
        SelectObject(hdc, old);
    }
}

/// Route a WM_NOTIFY custom-draw to the right painter.
pub fn on_notify(app: &mut App, lp: LPARAM) -> Option<isize> {
    let hdr = unsafe { &*(lp.0 as *const windows::Win32::UI::Controls::NMHDR) };
    if hdr.code != windows::Win32::UI::Controls::NM_CUSTOMDRAW {
        return None;
    }
    let header_wnd = app.header_hwnd();
    if hdr.hwndFrom == header_wnd {
        let draw = unsafe { &*(lp.0 as *const NMCUSTOMDRAW) };
        header(app, draw)
    } else if hdr.hwndFrom == app.ui.list {
        let draw = unsafe { &mut *(lp.0 as *mut NMLVCUSTOMDRAW) };
        list(app, draw)
    } else {
        None
    }
}

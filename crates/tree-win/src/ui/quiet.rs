//! Quiet-hours window: a small modal with two HH:MM fields.
//!
//! A scheduler is the feature most often missing from a download manager, but it
//! only needs a time window to be useful: pause everything inside it, resume the
//! tasks this feature paused when it closes.

use crate::app::App;
use crate::win32::{controls, theme, wide, window};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::UpdateWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

const ID_START: i32 = 1201;
const ID_END: i32 = 1202;
const ID_LABEL_START: i32 = 1203;
const ID_LABEL_END: i32 = 1204;
const ID_HINT: i32 = 1205;
const ID_OK: i32 = 1206;
const ID_CANCEL: i32 = 1207;

pub fn open(app: &mut App) {
    unsafe {
        if !app.quiet_dlg.is_invalid() {
            let _ = SetForegroundWindow(app.quiet_dlg);
            return;
        }
        let _ = EnableWindow(app.hwnd, false);
        let created = window::create(
            window::QUIET_CLASS,
            "Quiet hours",
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
            WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT,
            None,
            app.s(420),
            app.s(210),
        );
        let Ok(dlg) = created else {
            let _ = EnableWindow(app.hwnd, true);
            return;
        };
        app.quiet_dlg = dlg;

        let font = app.font;
        let label_start = controls::label(dlg, ID_LABEL_START, "Pause downloads at", 0, 0, 10, 10);
        let start = controls::edit(dlg, ID_START, false, 0, 0, 10, 10);
        let label_end = controls::label(dlg, ID_LABEL_END, "Resume downloads at", 0, 0, 10, 10);
        let end = controls::edit(dlg, ID_END, false, 0, 0, 10, 10);
        let hint = controls::label(
            dlg,
            ID_HINT,
            "Tasks already running are paused, then resumed when the window closes.",
            0,
            0,
            10,
            10,
        );
        let ok = controls::default_button(dlg, ID_OK, "Save", 0, 0, 10, 10);
        let cancel = controls::button(dlg, ID_CANCEL, "Cancel", 0, 0, 10, 10);
        for child in [label_start, start, label_end, end, hint, ok, cancel] {
            controls::set_font(child, font);
            theme::control(child, app.palette.colors.dark);
        }
        for button in [ok, cancel] {
            controls::set_owner_draw(button, app.palette.colors.dark);
        }
        wide::set_text(start, &format_minutes(app.settings.quiet_start));
        wide::set_text(end, &format_minutes(app.settings.quiet_end));

        layout(app, dlg);
        center_on(app, dlg);
        theme::frame(dlg, app.palette.colors.dark);
        let _ = ShowWindow(dlg, SW_SHOW);
        let _ = UpdateWindow(dlg);
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(start));
        SendMessageW(
            start,
            windows::Win32::UI::Controls::EM_SETSEL,
            Some(WPARAM(0)),
            Some(LPARAM(-1)),
        );
    }
}

fn format_minutes(minutes: u16) -> String {
    format!("{:02}:{:02}", (minutes / 60) % 24, minutes % 60)
}

/// Accept "2:00", "02:00", "0200" and "2". Returns minutes since midnight.
fn parse_minutes(text: &str) -> Option<u16> {
    let text = text.trim().replace('.', ":");
    if text.is_empty() {
        return None;
    }
    if let Some((h, m)) = text.split_once(':') {
        let hour: u16 = h.trim().parse().ok()?;
        let minute: u16 = m.trim().parse().ok()?;
        return (hour < 24 && minute < 60).then_some(hour * 60 + minute);
    }
    if text.len() == 4 && text.chars().all(|c| c.is_ascii_digit()) {
        let hour: u16 = text[..2].parse().ok()?;
        let minute: u16 = text[2..].parse().ok()?;
        return (hour < 24 && minute < 60).then_some(hour * 60 + minute);
    }
    let hour: u16 = text.parse().ok()?;
    (hour < 24).then_some(hour * 60)
}

fn layout(app: &App, dlg: HWND) {
    let pad = app.s(16);
    let label_w = app.s(150);
    let edit_w = app.s(90);
    let h = app.s(24);
    let row = app.s(34);
    let mut y = pad;

    let a = unsafe { GetDlgItem(Some(dlg), ID_LABEL_START).unwrap_or_default() };
    window::place(a, pad, y + app.s(4), label_w, h);
    let b = unsafe { GetDlgItem(Some(dlg), ID_START).unwrap_or_default() };
    window::place(b, pad + label_w + app.s(8), y, edit_w, h);
    y += row;

    let c = unsafe { GetDlgItem(Some(dlg), ID_LABEL_END).unwrap_or_default() };
    window::place(c, pad, y + app.s(4), label_w, h);
    let d = unsafe { GetDlgItem(Some(dlg), ID_END).unwrap_or_default() };
    window::place(d, pad + label_w + app.s(8), y, edit_w, h);
    y += row;

    let hint = unsafe { GetDlgItem(Some(dlg), ID_HINT).unwrap_or_default() };
    window::place(hint, pad, y, app.s(380), app.s(34));
    y += app.s(42);

    let bw = app.s(92);
    let ok = unsafe { GetDlgItem(Some(dlg), ID_OK).unwrap_or_default() };
    window::place(ok, app.s(420) - pad - bw, y, bw, app.s(28));
    let cancel = unsafe { GetDlgItem(Some(dlg), ID_CANCEL).unwrap_or_default() };
    window::place(cancel, app.s(420) - pad - bw * 2 - app.s(8), y, bw, app.s(28));
}

fn center_on(app: &App, dlg: HWND) {
    unsafe {
        let mut parent = RECT::default();
        let mut own = RECT::default();
        let _ = GetWindowRect(app.hwnd, &mut parent);
        let _ = GetWindowRect(dlg, &mut own);
        let w = own.right - own.left;
        let h = own.bottom - own.top;
        let x = parent.left + ((parent.right - parent.left) - w) / 2;
        let y = parent.top + ((parent.bottom - parent.top) - h) / 2;
        let _ = SetWindowPos(
            dlg,
            None,
            x,
            y,
            w,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn close(app: &mut App) {
    let dlg = app.quiet_dlg;
    if dlg.is_invalid() {
        return;
    }
    app.quiet_dlg = HWND::default();
    unsafe {
        let _ = DestroyWindow(dlg);
        let _ = EnableWindow(app.hwnd, true);
        let _ = SetForegroundWindow(app.hwnd);
    }
}

fn commit(app: &mut App) {
    let dlg = app.quiet_dlg;
    if dlg.is_invalid() {
        return;
    }
    let start_box = unsafe { GetDlgItem(Some(dlg), ID_START).unwrap_or_default() };
    let end_box = unsafe { GetDlgItem(Some(dlg), ID_END).unwrap_or_default() };
    let start = parse_minutes(&wide::get_text(start_box));
    let end = parse_minutes(&wide::get_text(end_box));
    let (Some(start), Some(end)) = (start, end) else {
        unsafe {
            MessageBoxW(
                Some(dlg),
                w!("Use 24-hour times like 02:00 and 08:00."),
                w!("Quiet hours"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        return;
    };
    if start == end {
        unsafe {
            MessageBoxW(
                Some(dlg),
                w!("The window needs two different times."),
                w!("Quiet hours"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        return;
    }
    app.settings.quiet_start = start;
    app.settings.quiet_end = end;
    app.settings.quiet_enabled = true;
    app.save_settings();
    app.apply_quiet_hours_now();
    close(app);
}

pub unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = crate::APP.load(std::sync::atomic::Ordering::Relaxed);
    let app = if ptr.is_null() {
        None
    } else {
        Some(unsafe { &mut *ptr })
    };

    match msg {
        WM_COMMAND => {
            if let Some(app) = app {
                let id = (wp.0 & 0xFFFF) as i32;
                let code = ((wp.0 >> 16) & 0xFFFF) as u32;
                if code == 0 || code == 1 {
                    match id {
                        ID_OK => commit(app),
                        ID_CANCEL => close(app),
                        _ => {}
                    }
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(app) = app {
                close(app);
            }
            LRESULT(0)
        }
        WM_DRAWITEM => {
            if let Some(app) = app {
                unsafe {
                    let dis = &*(lp.0 as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT);
                    let text = wide::get_text(dis.hwndItem);
                    let pressed = dis.itemState.0 & 1 != 0;
                    let primary = dis.CtlID == controls::ID_OK as u32;
                    controls::paint_owner_button(
                        dis.hDC,
                        &dis.rcItem,
                        &text,
                        pressed,
                        primary,
                        app.palette.colors,
                    );
                }
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT => {
            if let Some(app) = app {
                return app.on_ctlcolor_any(wp);
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_KEYDOWN => {
            if wp.0 as u32 == 0x1B {
                if let Some(app) = app {
                    close(app);
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

#[cfg(test)]
mod tests {
    use super::parse_minutes;

    #[test]
    fn reads_times_in_the_shapes_people_type() {
        assert_eq!(parse_minutes("02:00"), Some(120));
        assert_eq!(parse_minutes("2:30"), Some(150));
        assert_eq!(parse_minutes(" 23:15 "), Some(1395));
        assert_eq!(parse_minutes("0200"), Some(120));
        assert_eq!(parse_minutes("7"), Some(420));
        assert_eq!(parse_minutes("24:00"), None);
        assert_eq!(parse_minutes("02:75"), None);
        assert_eq!(parse_minutes(""), None);
    }
}

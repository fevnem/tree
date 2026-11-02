//! The Add dialog: a real modal window (owner disabled, Tab order, Esc/Ctrl+Enter).

use super::strings;
use crate::app::App;
use crate::win32::{controls, picker, theme, wide, window};
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::UpdateWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

pub fn open(app: &mut App) {
    unsafe {
        if !app.add_dlg.is_invalid() {
            let _ = SetForegroundWindow(app.add_dlg);
            return;
        }
        let _ = EnableWindow(app.hwnd, false);
        let created = window::create(
            window::ADD_CLASS,
            strings::ADD_TITLE,
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
            WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT,
            None,
            app.s(560),
            app.s(300),
        );
        let Ok(dlg) = created else {
            let _ = EnableWindow(app.hwnd, true);
            return;
        };
        app.add_dlg = dlg;

        let font = app.font;
        let label_url = controls::label(dlg, controls::ID_LABEL_URL, strings::ADD_URL, 0, 0, 10, 10);
        let label_dir = controls::label(dlg, controls::ID_LABEL_DIR, strings::ADD_DIR, 0, 0, 10, 10);
        let url = controls::edit(dlg, controls::ID_URL, true, 0, 0, 10, 10);
        let dir = controls::edit(dlg, controls::ID_DIR, false, 0, 0, 10, 10);
        let browse = controls::button(dlg, controls::ID_BROWSE, strings::ADD_BROWSE, 0, 0, 10, 10);
        let ok = controls::default_button(dlg, controls::ID_OK, strings::ADD_OK, 0, 0, 10, 10);
        let cancel = controls::button(dlg, controls::ID_CANCEL, strings::ADD_CANCEL, 0, 0, 10, 10);
        for child in [label_url, label_dir, url, dir, browse, ok, cancel] {
            controls::set_font(child, font);
        }
        wide::set_text(dir, &app.settings.dir);

        // Pre-fill from the clipboard: the common case is "copy link, press Ctrl+N".
        if let Some(link) = crate::clipboard::first_url(&crate::clipboard::read_text().unwrap_or_default())
        {
            wide::set_text(url, &link);
        }

        layout(app, dlg);
        center_on(app, dlg);
        theme::frame(dlg, app.palette.colors.dark);
        let _ = ShowWindow(dlg, SW_SHOW);
        let _ = UpdateWindow(dlg);
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(url));
        SendMessageW(
            url,
            windows::Win32::UI::Controls::EM_SETSEL,
            Some(WPARAM(0)),
            Some(LPARAM(-1)),
        );
    }
}

pub fn commit(app: &mut App) {
    let dlg = app.add_dlg;
    crate::dbg(&format!("commit: dlg valid={}", !dlg.is_invalid()));
    if dlg.is_invalid() {
        return;
    }
    let url_box = unsafe { GetDlgItem(Some(dlg), controls::ID_URL).unwrap_or_default() };
    let dir_box = unsafe { GetDlgItem(Some(dlg), controls::ID_DIR).unwrap_or_default() };
    let text = wide::get_text(url_box);
    let dir = wide::get_text(dir_box);

    crate::dbg(&format!(
        "commit: text_len={} dir={:?} first_line={:?}",
        text.len(),
        dir,
        text.lines().next().unwrap_or("")
    ));
    let urls: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("http://") || l.starts_with("https://"))
        .collect();

    crate::dbg(&format!("commit: urls={}", urls.len()));
    if urls.is_empty() {
        unsafe {
            MessageBoxW(
                Some(dlg),
                w!("A link must start with http:// or https://"),
                w!("Nothing to add"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        return;
    }
    if !dir.trim().is_empty() && dir.trim() != app.settings.dir {
        app.settings.dir = dir.trim().to_string();
        app.save_settings();
    }
    app.engine.add_many(&urls);
    crate::dbg("commit: add_many done");
    app.refresh();
    close(app);
    crate::dbg("commit: closed");
}

pub fn browse(app: &mut App) {
    let dlg = app.add_dlg;
    if dlg.is_invalid() {
        return;
    }
    if let Some(folder) = picker::pick_folder(dlg, strings::PICK_FOLDER) {
        if let Ok(field) = unsafe { GetDlgItem(Some(dlg), controls::ID_DIR) } {
            wide::set_text(field, &folder);
        }
    }
}

pub fn close(app: &mut App) {
    let dlg = app.add_dlg;
    if dlg.is_invalid() {
        return;
    }
    app.add_dlg = HWND::default();
    unsafe {
        let _ = DestroyWindow(dlg);
        let _ = EnableWindow(app.hwnd, true);
        let _ = SetForegroundWindow(app.hwnd);
    }
    app.focus_list();
}

fn layout(app: &App, dlg: HWND) {
    let rect = window::client_rect(dlg);
    let w = (rect.right - rect.left).max(1);
    let h = (rect.bottom - rect.top).max(1);
    let pad = app.s(12);
    let label_h = app.s(16);
    let row = app.s(26);
    let gap = app.s(8);

    let url_label_y = pad;
    let url_y = url_label_y + label_h + app.s(4);
    let url_h = app.s(84);
    let dir_label_y = url_y + url_h + gap;
    let dir_y = dir_label_y + label_h + app.s(4);
    let button_y = h - pad - row;

    place_named(dlg, controls::ID_LABEL_URL, pad, url_label_y, w - pad * 2, label_h);
    place_named(dlg, controls::ID_URL, pad, url_y, w - pad * 2, url_h);
    place_named(dlg, controls::ID_LABEL_DIR, pad, dir_label_y, w - pad * 2, label_h);
    place_named(
        dlg,
        controls::ID_DIR,
        pad,
        dir_y,
        w - pad * 2 - app.s(96) - app.s(8),
        row,
    );
    place_named(dlg, controls::ID_BROWSE, w - pad - app.s(92), dir_y - app.s(2), app.s(92), row + app.s(4));
    place_named(dlg, controls::ID_OK, w - pad - app.s(184), button_y, app.s(88), row);
    place_named(dlg, controls::ID_CANCEL, w - pad - app.s(88), button_y, app.s(88), row);
}

fn place_named(dlg: HWND, id: i32, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        if let Ok(control) = GetDlgItem(Some(dlg), id) {
            let _ = MoveWindow(control, x, y, w, h, true);
        }
    }
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
        let _ = SetWindowPos(dlg, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

pub unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = crate::APP.load(std::sync::atomic::Ordering::Relaxed);
    if ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    }
    let app = unsafe { &mut *ptr };

    if msg == WM_COMMAND && ((wp.0 >> 16) & 0xFFFF) == 0 {
        crate::dbg(&format!("dialog button id={}", wp.0 & 0xFFFF));
    }

    match msg {
        WM_SIZE => {
            layout(app, hwnd);
            LRESULT(0)
        }
        // The buttons live in this window: without this arm Add/Cancel/Browse
        // send their notifications into a void.
        WM_COMMAND => {
            let id = (wp.0 & 0xFFFF) as i32;
            let code = ((wp.0 >> 16) & 0xFFFF) as u32;
            if code == 0 || code == 1 {
                match id {
                    controls::ID_OK => commit(app),
                    controls::ID_CANCEL => close(app),
                    controls::ID_BROWSE => browse(app),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_DRAWITEM => {
            app.on_draw_item(lp);
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => {
            let hdc = windows::Win32::Graphics::Gdi::HDC(wp.0 as *mut core::ffi::c_void);
            unsafe {
                windows::Win32::Graphics::Gdi::SetBkMode(
                    hdc,
                    windows::Win32::Graphics::Gdi::TRANSPARENT,
                );
                windows::Win32::Graphics::Gdi::SetTextColor(hdc, app.palette.text);
            }
            LRESULT(app.palette.background.0 as isize)
        }
        WM_ERASEBKGND => {
            let hdc = windows::Win32::Graphics::Gdi::HDC(wp.0 as *mut core::ffi::c_void);
            let rect = window::client_rect(hwnd);
            unsafe {
                windows::Win32::Graphics::Gdi::FillRect(hdc, &rect, app.palette.background);
            }
            LRESULT(1)
        }
        WM_CLOSE => {
            close(app);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

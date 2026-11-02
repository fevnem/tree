//! Tree entry point: COM, common controls, single instance, message loop.

#![windows_subsystem = "windows"]

mod app;
mod clipboard;
mod paths;
mod single;
mod ui;
mod win32;

use std::sync::atomic::{AtomicPtr, Ordering};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_BAR_CLASSES, ICC_LISTVIEW_CLASSES, ICC_PROGRESS_CLASS,
    ICC_STANDARD_CLASSES, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, VK_CONTROL, VK_DELETE, VK_ESCAPE, VK_F5, VK_RETURN, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::*;

pub static APP: AtomicPtr<app::App> = AtomicPtr::new(std::ptr::null_mut());

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES
                | ICC_LISTVIEW_CLASSES
                | ICC_PROGRESS_CLASS
                | ICC_BAR_CLASSES
                | ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    // A second launch hands its link to the window that is already open.
    if single::forward(&args) {
        return;
    }

    let mut app = app::App::new();
    APP.store(&mut *app as *mut app::App, Ordering::Relaxed);
    if app.create_window().is_err() {
        return;
    }
    if !args.is_empty() {
        app.engine.add_many(&args);
        app.refresh();
    }
    message_loop();
}

fn message_loop() {
    let mut msg = MSG::default();
    loop {
        let alive = unsafe { GetMessageW(&mut msg, None, 0, 0).as_bool() };
        if !alive {
            return;
        }
        if preprocess(&msg) {
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Keyboard shortcuts, applied before dispatch so they work wherever focus is
/// (except inside a text field, which must keep its own keys).
fn preprocess(msg: &MSG) -> bool {
    let ptr = APP.load(Ordering::Relaxed);
    if ptr.is_null() || msg.message != WM_KEYDOWN {
        return false;
    }
    let app = unsafe { &mut *ptr };
    let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
    let key = msg.wParam.0 as u16;
    let keyboard = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;

    if !app.add_dlg.is_invalid() {
        if key == VK_ESCAPE.0 {
            ui::add_dialog::close(app);
            return true;
        }
        if key == VK_RETURN.0 && ctrl {
            ui::add_dialog::commit(app);
            return true;
        }
        return false;
    }

    let focus = unsafe { GetFocus() };
    let in_text_field = focus == app.ui.filter;

    if ctrl && key == 'N' as u16 {
        app.run_id(win32::menu::ID_ADD);
        return true;
    }
    if ctrl && key == 'F' as u16 {
        unsafe {
            let _ = keyboard(Some(app.ui.filter));
        }
        return true;
    }
    if ctrl && key == 'V' as u16 && !in_text_field {
        if let Some(url) = clipboard::first_url(&clipboard::read_text().unwrap_or_default()) {
            app.engine.add(&url);
            app.refresh();
        }
        return true;
    }
    if in_text_field {
        return false;
    }
    if key == VK_DELETE.0 {
        app.run_id(win32::menu::ID_REMOVE);
        return true;
    }
    if key == VK_SPACE.0 {
        let ids = app.selected_ids();
        let running = app
            .tasks
            .iter()
            .filter(|t| ids.contains(&t.id))
            .any(|t| t.status == tree_core::Status::Running);
        app.run_id(if running {
            win32::menu::ID_PAUSE
        } else {
            win32::menu::ID_RESUME
        });
        return true;
    }
    if key == VK_RETURN.0 {
        app.run_id(win32::menu::ID_OPEN);
        return true;
    }
    if key == VK_F5.0 {
        app.run_id(win32::menu::ID_RESUME);
        return true;
    }
    if key == VK_ESCAPE.0 {
        app.focus_list();
    }
    false
}

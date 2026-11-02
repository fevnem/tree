//! Right-click menu for the selected rows.

use crate::app::App;
use crate::win32::listview;
use crate::win32::menu;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, SetForegroundWindow, TrackPopupMenuEx, TPM_RETURNCMD,
    TPM_RIGHTBUTTON,
};

pub fn show(app: &mut App) {
    // Select whatever the user right-clicked, if nothing there is selected yet.
    let selected = app.selected_ids();
    if selected.is_empty() {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
            let _ = ScreenToClient(app.hwnd, &mut pt);
            let row = listview::hit_test(app.ui.list, pt.x, pt.y);
            if row >= 0 {
                listview::focus_row(app.ui.list, row);
            }
        }
    }

    let ids = app.selected_ids();
    if ids.is_empty() {
        return;
    }
    let any_running = app
        .tasks
        .iter()
        .filter(|t| ids.contains(&t.id))
        .any(|t| matches!(t.status, tree_core::Status::Running | tree_core::Status::Queued));
    let any_paused = app
        .tasks
        .iter()
        .filter(|t| ids.contains(&t.id))
        .any(|t| matches!(t.status, tree_core::Status::Paused | tree_core::Status::Failed));

    let Some(popup) = menu::row_menu(any_running, any_paused) else {
        return;
    };
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(app.hwnd);
        let cmd = TrackPopupMenuEx(
            popup,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
            pt.x,
            pt.y,
            app.hwnd,
            None,
        );
        let _ = windows::Win32::UI::WindowsAndMessaging::DestroyMenu(popup);
        let id = cmd.0 as usize;
        if id != 0 {
            app.run_id(id);
        }
    }
}

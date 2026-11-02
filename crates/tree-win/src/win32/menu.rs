//! The native menu bar. One menu tree, rebuilt when settings change so the
//! check marks are always true.

use super::theme;
use super::wide;
use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateMenu, CreatePopupMenu, HMENU, MENU_ITEM_FLAGS, MF_CHECKED, MF_POPUP,
    MF_SEPARATOR, MF_STRING,
};

// Same command ids as the toolbar buttons where the action is identical.
pub const ID_ADD: usize = 1001;
pub const ID_PAUSE: usize = 1002;
pub const ID_RESUME: usize = 1003;
pub const ID_REMOVE: usize = 1004;
pub const ID_FOLDER: usize = 1005;

pub const ID_OPEN: usize = 1100;
pub const ID_REMOVE_FILE: usize = 1101;
pub const ID_COPY_URL: usize = 1102;
pub const ID_PAUSE_ALL: usize = 1103;
pub const ID_RESUME_ALL: usize = 1104;
pub const ID_EXIT: usize = 1105;
pub const ID_ABOUT: usize = 1106;
pub const ID_WATCH: usize = 1107;
pub const ID_TRAY: usize = 1108;
pub const ID_SHOW: usize = 1109;
pub const ID_YTDLP: usize = 1110;

pub const ID_CONNS_BASE: usize = 1200;
pub const ID_SPEED_BASE: usize = 1300;
pub const ID_THEME_BASE: usize = 1400;

fn item(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str) {
    let t = wide::wide(text);
    unsafe {
        let _ = AppendMenuW(menu, flags, id, wide::pcw(&t));
    }
}

fn separator(menu: HMENU) {
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    }
}

fn submenu(bar: HMENU, menu: HMENU, text: &str) {
    let t = wide::wide(text);
    unsafe {
        let _ = AppendMenuW(bar, MF_POPUP, menu.0 as usize, wide::pcw(&t));
    }
}

fn check(on: bool) -> MENU_ITEM_FLAGS {
    if on {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    }
}

/// Build the whole bar for the current settings.
pub fn build(conns: usize, speed_limit: u64, watch: bool, tray: bool, theme_name: &str) -> Option<HMENU> {
    unsafe {
        let bar = CreateMenu().ok()?;

        let file = CreatePopupMenu().ok()?;
        item(file, MF_STRING, ID_ADD, "&Add URL…\tCtrl+N");
        item(file, MF_STRING, ID_FOLDER, "Open download &folder");
        separator(file);
        item(file, MF_STRING, ID_EXIT, "E&xit\tAlt+F4");
        submenu(bar, file, "&File");

        let dl = CreatePopupMenu().ok()?;
        item(dl, MF_STRING, ID_OPEN, "&Open file\tEnter");
        item(dl, MF_STRING, ID_COPY_URL, "Copy &URL");
        separator(dl);
        item(dl, MF_STRING, ID_PAUSE, "&Pause\tSpace");
        item(dl, MF_STRING, ID_RESUME, "&Resume\tSpace");
        item(dl, MF_STRING, ID_PAUSE_ALL, "Pause a&ll");
        item(dl, MF_STRING, ID_RESUME_ALL, "Resume a&ll");
        separator(dl);
        item(dl, MF_STRING, ID_REMOVE, "Re&move\tDel");
        item(dl, MF_STRING, ID_REMOVE_FILE, "Remove and delete &file");
        submenu(bar, dl, "&Downloads");

        let view = CreatePopupMenu().ok()?;
        let conns_menu = CreatePopupMenu().ok()?;
        for n in [1usize, 2, 4, 8, 16] {
            item(
                conns_menu,
                check(conns == n),
                ID_CONNS_BASE + n,
                &format!("{} connection{}", n, if n == 1 { "" } else { "s" }),
            );
        }
        submenu(view, conns_menu, "&Connections per file");

        let speed = CreatePopupMenu().ok()?;
        for (label, value) in [
            ("&Unlimited", 0u64),
            ("1 MB/s", 1 << 20),
            ("5 MB/s", 5 << 20),
            ("10 MB/s", 10 << 20),
            ("25 MB/s", 25 << 20),
        ] {
            item(
                speed,
                check(speed_limit == value),
                ID_SPEED_BASE + (value >> 20) as usize,
                label,
            );
        }
        submenu(view, speed, "Speed &limit");

        // Ten themes, all reachable from the same submenu.
        let themes = CreatePopupMenu().ok()?;
        for (i, (key, label)) in theme::THEMES.iter().enumerate() {
            item(themes, check(theme_name == *key), ID_THEME_BASE + i, label);
        }
        submenu(view, themes, "&Theme");

        separator(view);
        item(view, check(watch), ID_WATCH, "Watch &clipboard for links");
        item(view, check(tray), ID_TRAY, "Close to the &tray");
        submenu(bar, view, "&View");

        let tools = CreatePopupMenu().ok()?;
        item(tools, MF_STRING, ID_YTDLP, "Download &yt-dlp (video support)…");
        submenu(bar, tools, "&Tools");

        let help = CreatePopupMenu().ok()?;
        item(help, MF_STRING, ID_ABOUT, "&About Tree");
        submenu(bar, help, "&Help");

        Some(bar)
    }
}

/// Right-click menu for one or more selected rows.
pub fn row_menu(any_running: bool, any_paused: bool) -> Option<HMENU> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        item(menu, MF_STRING, ID_OPEN, "&Open file");
        item(menu, MF_STRING, ID_FOLDER, "Open &folder");
        item(menu, MF_STRING, ID_COPY_URL, "Copy &URL");
        separator(menu);
        if any_running {
            item(menu, MF_STRING, ID_PAUSE, "&Pause");
        }
        if any_paused {
            item(menu, MF_STRING, ID_RESUME, "&Resume");
        }
        separator(menu);
        item(menu, MF_STRING, ID_REMOVE, "Re&move from list");
        item(menu, MF_STRING, ID_REMOVE_FILE, "Remove and &delete file");
        Some(menu)
    }
}

pub fn tray_menu(has_tasks: bool) -> Option<HMENU> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        item(menu, MF_STRING, ID_SHOW, "Open &Tree");
        item(menu, MF_STRING, ID_ADD, "&Add link…");
        separator(menu);
        if has_tasks {
            item(menu, MF_STRING, ID_PAUSE_ALL, "Pause all");
            item(menu, MF_STRING, ID_RESUME_ALL, "Resume all");
            separator(menu);
        }
        item(menu, MF_STRING, ID_EXIT, "E&xit");
        Some(menu)
    }
}

pub fn destroy(menu: HMENU) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::DestroyMenu(menu);
    }
}

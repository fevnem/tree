//! Application state and the window procedure. Every user action funnels through
//! `run_id`, so the toolbar, the menu, the tray and the context menu cannot
//! drift apart.

use crate::bindings;
use crate::paths;
use crate::single;
use crate::ui::{add_dialog, context_menu, format, shell, strings};
use crate::win32::{controls, listview, menu, theme, wide, window};
use std::sync::atomic::Ordering;
use tree_core::{Engine, Settings, Status, Task};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, EndPaint, FillRect, InvalidateRect, PAINTSTRUCT, SetBkColor, SetBkMode,
    SetTextColor, UpdateWindow, HDC, HGDIOBJ, HFONT, TRANSPARENT,
};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Controls::{NMHDR, NM_DBLCLK, NM_RCLICK};
use windows::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, ITaskbarList3, NIF_ICON, NIF_MESSAGE, NIF_TIP,
    NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, ShellExecuteW, Shell_NotifyIconW,
    TaskbarList, TBPF_ERROR, TBPF_NORMAL, TBPF_NOPROGRESS, TBPF_PAUSED, HDROP,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_TRAY: u32 = WM_APP + 1;
/// winuser.h value; the crate does not export it, and an undefined name here
/// would silently become a catch-all match arm.
pub const WM_MOUSELEAVE: u32 = 0x02A3;

/// Local time as minutes since midnight.
pub fn local_minute() -> u16 {
    // GetLocalTime returns the SYSTEMTIME directly in this crate version.
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    (st.wHour as u16) * 60 + st.wMinute as u16
}

fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

/// What the pointer is over in our own chrome.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hot {
    Tool(usize),
    Nav(usize),
    Cat(usize),
    Empty,
}
pub const WM_TOOLS_DONE: u32 = WM_APP + 2;
pub const TIMER_UI: usize = 1;
pub const TRAY_ID: u32 = 1;

pub struct Ui {
    pub list: HWND,
    pub filter: HWND,
}

pub struct App {
    pub hwnd: HWND,
    pub engine: Engine,
    pub ui: Ui,
    pub settings: Settings,
    pub tasks: Vec<Task>,
    pub rows: Vec<u64>,
    pub font: HFONT,
    pub icon: HICON,
    pub small_icon: HICON,
    pub palette: theme::Palette,
    pub dpi: u32,
    pub hmenu: Option<HMENU>,
    pub tray: bool,
    pub taskbar_created: u32,
    pub taskbar: Option<ITaskbarList3>,
    pub last_clip: String,
    pub status_buf: [[u16; 512]; 3],
    pub ticks: u32,
    pub add_dlg: HWND,
    /// Sidebar selection and hover, both owned by `ui::shell`.
    pub filter: crate::ui::shell::Filter,
    pub hover: Option<Hot>,
    pub counts: [usize; 6],
    /// Per-category counts for the FILE TYPES section.
    pub cat_counts: [usize; 7],
    pub category: crate::ui::shell::Category,
    /// Total throughput samples for the status sparkline.
    pub speed_hist: std::collections::VecDeque<u64>,
    pub tracking: bool,
    /// Keep the selection on the active transfer until the user picks a row.
    pub follow: bool,
    /// Tasks the quiet-hours window paused, so it only resumes its own work.
    pub paused_by_quiet: Vec<u64>,
    pub quiet_dlg: HWND,
    /// Loopback bridge for the browser extension, drained on the UI timer.
    pub bridge: Option<std::sync::mpsc::Receiver<Vec<String>>>,
}

fn scale(dpi: u32, v: i32) -> i32 {
    (v * dpi as i32 + 48) / 96
}

impl App {
    pub fn new() -> Box<App> {
        let engine = Engine::start(paths::state_path(), paths::downloads_dir());
        let mut settings = engine.settings();
        settings.sane_columns();
        let palette = theme::Palette::new(theme::palette_for(&settings.theme));
        let hmenu = menu::build(
            settings.conns,
            settings.speed_limit,
            settings.watch_clipboard,
            settings.minimize_to_tray,
            settings.quiet_enabled,
            &settings.theme,
        );
        Box::new(App {
            hwnd: HWND::default(),
            engine,
            ui: Ui {
                list: HWND::default(),
                filter: HWND::default(),
            },
            font: theme::message_font(),
            small_icon: load_icon(true),
            icon: load_icon(false),
            palette,
            settings,
            tasks: Vec::new(),
            rows: Vec::new(),
            dpi: 96,
            hmenu,
            tray: false,
            taskbar_created: 0,
            taskbar: None,
            last_clip: String::new(),
            status_buf: [[0; 512]; 3],
            ticks: 0,
            add_dlg: HWND::default(),
            filter: crate::ui::shell::Filter::All,
            hover: None,
            counts: [0; 6],
            cat_counts: [0; 7],
            category: crate::ui::shell::Category::All,
            speed_hist: std::collections::VecDeque::new(),
            tracking: false,
            follow: true,
            paused_by_quiet: Vec::new(),
            quiet_dlg: HWND::default(),
            bridge: bindings::start(bindings::PORT),
        })
    }

    pub fn s(&self, v: i32) -> i32 {
        scale(self.dpi, v)
    }

    pub fn create_window(&mut self) -> windows::core::Result<()> {
        unsafe {
            window::register_class(
                window::MAIN_CLASS,
                Some(self.icon),
                Some(wndproc),
                self.palette.background,
            );
            window::register_class(
                window::ADD_CLASS,
                Some(self.icon),
                Some(add_dialog::wndproc),
                self.palette.background,
            );
            window::register_class(
                window::QUIET_CLASS,
                Some(self.icon),
                Some(crate::ui::quiet::wndproc),
                self.palette.background,
            );
            let hwnd = window::create(
                window::MAIN_CLASS,
                strings::WINDOW_TITLE,
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                WS_EX_APPWINDOW,
                None,
                self.s(if self.settings.win_w > 0 { self.settings.win_w } else { 980 }),
                self.s(if self.settings.win_h > 0 { self.settings.win_h } else { 620 }),
            )?;
            self.hwnd = hwnd;
            self.dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96);

            // Restore the last window position when it is still on a monitor.
            if self.settings.win_w > 0 && self.settings.win_h > 0 {
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                let w = rect.right - rect.left;
                let h = rect.bottom - rect.top;
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    self.settings.win_x,
                    self.settings.win_y,
                    w,
                    h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }

            if let Some(hmenu) = self.hmenu {
                let _ = SetMenu(hwnd, Some(hmenu));
            }
            theme::frame(hwnd, self.palette.colors.dark);
            DragAcceptFiles(hwnd, true);
            self.taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));

            self.create_children();
            self.layout();

            let _ = SetTimer(Some(hwnd), TIMER_UI, 250, None);
            self.tray = self.tray_add();
            self.taskbar = self.taskbar_init();
            self.refresh();

            let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
            let _ = UpdateWindow(hwnd);
        }
        Ok(())
    }

    fn create_children(&mut self) {
        let h = self.hwnd;
        let list = listview::create(h, controls::ID_LIST, 0, 0, 10, 10);
        for (i, title) in strings::COLUMNS.iter().enumerate() {
            let width = self.settings.col_widths.get(i).copied().unwrap_or(120);
            listview::add_column(list, i as i32, title, self.s(width));
        }
        let filter = controls::edit(h, controls::ID_FILTER, false, 0, 0, 10, 10);
        cue_banner(filter, strings::FILTER_HINT);

        let font = self.font;
        for child in [list, filter] {
            controls::set_font(child, font);
        }

        self.ui = Ui { list, filter };
        self.apply_theme();
    }

    /// Push the current palette at every control we own. Dark palettes get our
    /// own button painting; light ones keep the stock look.
    pub fn apply_theme(&self) {
        let colors = self.palette.colors;
        theme::listview(self.ui.list, colors);
        theme::control(self.ui.filter, colors.dark);
        invalidate(self.hwnd);
    }

    pub fn header_hwnd(&self) -> HWND {
        let result = unsafe {
            SendMessageW(
                self.ui.list,
                windows::Win32::UI::Controls::LVM_GETHEADER,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            )
        };
        HWND(result.0 as *mut core::ffi::c_void)
    }

    /// `WM_DRAWITEM` from the owner-drawn table.
    pub fn on_draw_item(&self, lp: LPARAM) -> bool {
        unsafe {
            let dis = &*(lp.0 as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT);
            if dis.CtlID != controls::ID_LIST as u32 {
                return false;
            }
            let selected = dis.itemState.0 & 1 != 0;
            shell::draw_row(self, dis.hDC, dis.rcItem, dis.itemID as usize, selected);
            true
        }
    }

    pub fn set_category(&mut self, category: crate::ui::shell::Category) {
        if self.category != category {
            self.category = category;
            self.rows.clear();
            self.refresh();
        }
    }

    pub fn set_filter(&mut self, filter: crate::ui::shell::Filter) {
        if self.filter != filter {
            self.filter = filter;
            self.rows.clear();
            self.refresh();
        }
    }

    pub fn selected_task(&self) -> Option<&Task> {
        let ids = self.selected_ids();
        self.tasks.iter().find(|t| ids.contains(&t.id))
    }

    pub fn track_leave(&mut self) {
        if self.tracking {
            return;
        }
        self.tracking = true;
        let mut event = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.hwnd,
            dwHoverTime: 0,
        };
        unsafe {
            let _ = TrackMouseEvent(&mut event);
        }
    }

    // ---- commands -------------------------------------------------------

    pub fn run_id(&mut self, id: usize) {
        match id {
            menu::ID_ADD => add_dialog::open(self),
            menu::ID_PAUSE => {
                for id in self.selected() {
                    self.engine.pause(id);
                }
            }
            menu::ID_RESUME => {
                for id in self.selected() {
                    self.engine.resume(id);
                }
            }
            menu::ID_PAUSE_ALL => self.engine.pause_all(),
            menu::ID_RESUME_ALL => self.engine.resume_all(),
            menu::ID_REMOVE => {
                for id in self.selected() {
                    self.engine.remove(id, false);
                }
            }
            menu::ID_REMOVE_FILE => {
                let ids = self.selected();
                if let Some(task) = self.tasks.iter().find(|t| ids.contains(&t.id)) {
                    let answer = unsafe {
                        MessageBoxW(
                            Some(self.hwnd),
                            wide::pcw(&wide::wide(&strings::confirm_delete(&task.name))),
                            w!("Tree"),
                            MB_YESNO | MB_ICONWARNING,
                        )
                    };
                    if answer == IDYES {
                        for id in ids {
                            self.engine.remove(id, true);
                        }
                    }
                }
            }
            menu::ID_OPEN => self.open_selected(),
            menu::ID_FOLDER => self.folder_selected(),
            menu::ID_COPY_URL => {
                if let Some(task) = self.tasks.iter().find(|t| self.selected().contains(&t.id)) {
                    crate::clipboard::write_text(&task.url);
                }
            }
            menu::ID_SHOW => self.show_window(),
            menu::ID_EXIT => unsafe {
                let _ = DestroyWindow(self.hwnd);
            },
            menu::ID_ABOUT => unsafe {
                MessageBoxW(
                    Some(self.hwnd),
                    wide::pcw(&wide::wide(&strings::about())),
                    w!("About Tree"),
                    MB_OK | MB_ICONINFORMATION,
                );
            },
            menu::ID_WATCH => {
                self.settings.watch_clipboard = !self.settings.watch_clipboard;
                self.save_settings();
            }
            menu::ID_TRAY => {
                self.settings.minimize_to_tray = !self.settings.minimize_to_tray;
                self.save_settings();
            }
            menu::ID_YTDLP => self.install_ytdlp(),
            menu::ID_QUIET => {
                self.settings.quiet_enabled = !self.settings.quiet_enabled;
                self.save_settings();
            }
            menu::ID_QUIET_SET => {
                crate::ui::quiet::open(self);
            }
            _ => {
                let themes = menu::ID_THEME_BASE..menu::ID_THEME_BASE + theme::THEMES.len();
                if id > menu::ID_CONNS_BASE && id <= menu::ID_CONNS_BASE + 16 {
                    self.settings.conns = id - menu::ID_CONNS_BASE;
                } else if id > menu::ID_SPEED_BASE && id <= menu::ID_SPEED_BASE + 64 {
                    self.settings.speed_limit = ((id - menu::ID_SPEED_BASE) as u64) << 20;
                } else if themes.contains(&id) {
                    self.settings.theme = theme::THEMES[id - menu::ID_THEME_BASE].0.to_string();
                    self.retheme();
                }
                self.save_settings();
                self.layout();
            }
        }
        self.refresh();
    }

    /// Add links and put the selection on the newest one, so the user sees what
    /// just happened.
    pub fn add_many(&mut self, urls: &[String]) {
        self.follow = true;
        let before = self
            .engine
            .snapshot()
            .iter()
            .map(|t| t.id)
            .max()
            .unwrap_or(0);
        self.engine.add_many(urls);
        self.refresh();
        if let Some(row) = self.rows.iter().position(|id| *id > before) {
            unsafe {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(self.ui.list));
            }
            listview::focus_row(self.ui.list, row as i32);
            self.refresh();
        }
    }

    fn selected(&self) -> Vec<u64> {
        self.selected_ids()
    }

    fn show_window(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    fn open_selected(&mut self) {
        let ids = self.selected();
        if let Some(task) = self.tasks.iter().find(|t| ids.contains(&t.id)) {
            let path = task.path();
            if path.exists() {
                let file = wide::wide(&path.to_string_lossy());
                unsafe {
                    ShellExecuteW(
                        Some(self.hwnd),
                        w!("open"),
                        wide::pcw(&file),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    );
                }
            }
        }
    }

    fn folder_selected(&mut self) {
        let ids = self.selected();
        if let Some(task) = self.tasks.iter().find(|t| ids.contains(&t.id)) {
            let dir = wide::wide(&task.dir);
            unsafe {
                ShellExecuteW(
                    Some(self.hwnd),
                    w!("open"),
                    wide::pcw(&dir),
                    PCWSTR::null(),
                    PCWSTR::null(),
                    SW_SHOWNORMAL,
                );
            }
        }
    }

    pub fn save_settings(&mut self) {
        // Widths are stored in 96-dpi units: scaling them on save as well would
        // make the columns grow on every launch at 125 %/150 %.
        let dpi = self.dpi.max(96);
        let logical: Vec<i32> = (0..6)
            .map(|i| (self.column_width(i) * 96 / dpi as i32).max(48))
            .collect();
        self.settings.col_widths = logical;

        if let Some(old) = self.hmenu.take() {
            menu::destroy(old);
        }
        self.hmenu = menu::build(
            self.settings.conns,
            self.settings.speed_limit,
            self.settings.watch_clipboard,
            self.settings.minimize_to_tray,
            self.settings.quiet_enabled,
            &self.settings.theme,
        );
        if let Some(hmenu) = self.hmenu {
            unsafe {
                let _ = SetMenu(self.hwnd, Some(hmenu));
                let _ = DrawMenuBar(self.hwnd);
            }
        }
        self.engine.set_settings(self.settings.clone());
    }

    fn column_width(&self, i: i32) -> i32 {
        unsafe {
            SendMessageW(
                self.ui.list,
                windows::Win32::UI::Controls::LVM_GETCOLUMNWIDTH,
                Some(WPARAM(i as usize)),
                Some(LPARAM(0)),
            )
            .0 as i32
        }
    }

    // ---- notifications --------------------------------------------------

    pub fn on_notify(&mut self, lp: LPARAM) -> Option<isize> {
        unsafe {
            let hdr = &*(lp.0 as *const NMHDR);
            if hdr.code == windows::Win32::UI::Controls::NM_CUSTOMDRAW
                && hdr.hwndFrom == self.ui.list
            {
                let draw = &*(lp.0 as *const windows::Win32::UI::Controls::NMCUSTOMDRAW);
                if draw.dwDrawStage == windows::Win32::UI::Controls::CDDS_POSTPAINT {
                    shell::paint_list_gap(self, draw.hdc);
                }
                return Some(windows::Win32::UI::Controls::CDRF_DODEFAULT as isize);
            }
            // The header is stock, and stock headers do not follow a dark palette.
            if hdr.code == windows::Win32::UI::Controls::NM_CUSTOMDRAW
                && hdr.hwndFrom == self.header_hwnd()
            {
                let draw = &*(lp.0 as *const windows::Win32::UI::Controls::NMCUSTOMDRAW);
                if draw.dwDrawStage == windows::Win32::UI::Controls::CDDS_PREPAINT {
                    shell::paint_header(self, draw.hdc);
                    return Some(windows::Win32::UI::Controls::CDRF_SKIPDEFAULT as isize);
                }
                return Some(windows::Win32::UI::Controls::CDRF_DODEFAULT as isize);
            }
            if hdr.code == windows::Win32::UI::Controls::NM_CLICK && hdr.hwndFrom == self.ui.list {
                // The user is driving now; stop moving the selection for them.
                self.follow = false;
            } else if hdr.code == windows::Win32::UI::Controls::LVN_COLUMNCLICK {
                let info = &*(lp.0 as *const windows::Win32::UI::Controls::NMLISTVIEW);
                let col = info.iSubItem;
                if self.settings.sort_col == col {
                    self.settings.sort_desc = !self.settings.sort_desc;
                } else {
                    self.settings.sort_col = col;
                    self.settings.sort_desc = false;
                }
                self.save_settings();
                self.refresh();
            } else if hdr.code == NM_DBLCLK {
                self.open_selected();
            } else if hdr.code == NM_RCLICK {
                context_menu::show(self);
            }
        }
        None
    }

    pub fn on_drop(&mut self, wp: WPARAM) {
        let hdrop = HDROP(wp.0 as *mut core::ffi::c_void);
        let mut urls = Vec::new();
        unsafe {
            let count = DragQueryFileW(hdrop, u32::MAX, None);
            for i in 0..count {
                let mut buf = [0u16; 1024];
                let n = DragQueryFileW(hdrop, i, Some(&mut buf));
                if n == 0 {
                    continue;
                }
                let path = String::from_utf16_lossy(&buf[..n as usize]);
                if path.starts_with("http://") || path.starts_with("https://") {
                    urls.push(path);
                } else if let Ok(text) = std::fs::read_to_string(&path) {
                    for line in text.lines() {
                        let line = line.trim().to_string();
                        if line.starts_with("http://") || line.starts_with("https://") {
                            urls.push(line);
                        }
                    }
                }
            }
            DragFinish(hdrop);
        }
        if !urls.is_empty() {
            self.add_many(&urls);
        }
    }

    pub fn on_timer(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        self.drain_bridge();
        self.apply_quiet_hours();
        self.refresh();
        if self.ticks.is_multiple_of(4) {
            self.poll_clipboard();
        }
        if self.ticks.is_multiple_of(8) {
            self.update_tray();
        }
    }

    /// Pause everything inside the quiet window and resume what we paused once
    /// the window closes. Only tasks this feature paused are resumed, so a
    /// manual pause is never undone.
    /// Re-run the window check right after the times change.
    pub fn apply_quiet_hours_now(&mut self) {
        self.paused_by_quiet.clear();
        self.apply_quiet_hours();
    }

    fn apply_quiet_hours(&mut self) {
        if !self.settings.quiet_enabled {
            if !self.paused_by_quiet.is_empty() {
                let ids = std::mem::take(&mut self.paused_by_quiet);
                for id in ids {
                    self.engine.resume(id);
                }
            }
            return;
        }
        let minute = local_minute();
        if self.settings.quiet_at(minute) {
            let running: Vec<u64> = self
                .engine
                .snapshot()
                .into_iter()
                .filter(|t| t.status == Status::Running || t.status == Status::Queued)
                .map(|t| t.id)
                .collect();
            for id in running {
                if !self.paused_by_quiet.contains(&id) {
                    self.paused_by_quiet.push(id);
                    self.engine.pause(id);
                }
            }
        } else if !self.paused_by_quiet.is_empty() {
            let ids = std::mem::take(&mut self.paused_by_quiet);
            for id in ids {
                self.engine.resume(id);
            }
        }
    }

    /// Links handed over by the browser extension.
    fn drain_bridge(&mut self) {
        let incoming: Vec<String> = match &self.bridge {
            Some(rx) => {
                let mut all = Vec::new();
                while let Ok(mut batch) = rx.try_recv() {
                    all.append(&mut batch);
                }
                all
            }
            None => return,
        };
        if !incoming.is_empty() {
            self.add_many(&incoming);
        }
    }

    fn poll_clipboard(&mut self) {
        if !self.settings.watch_clipboard {
            return;
        }
        let Some(text) = crate::clipboard::read_text() else {
            return;
        };
        if text == self.last_clip {
            return;
        }
        self.last_clip = text;
        if let Some(url) = crate::clipboard::first_url(&self.last_clip) {
            let known = self.tasks.iter().any(|t| t.url == url);
            if !known {
                self.engine.add(&url);
            }
        }
    }

    pub fn on_tray(&mut self, lp: LPARAM) {
        let event = (lp.0 as u32) & 0xFFFF;
        match event {
            WM_LBUTTONUP => {
                // Click toggles: restore when hidden, hide when already in front.
                let visible = unsafe { IsWindowVisible(self.hwnd).as_bool() };
                if visible && unsafe { GetForegroundWindow() } == self.hwnd {
                    unsafe {
                        let _ = ShowWindow(self.hwnd, SW_HIDE);
                    }
                } else {
                    self.show_window();
                }
            }
            WM_LBUTTONDBLCLK => self.show_window(),
            WM_RBUTTONUP => {
                let has_tasks = !self.tasks.is_empty();
                let Some(popup) = menu::tray_menu(has_tasks) else {
                    return;
                };
                let mut pt = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut pt);
                    // Required by the shell so the menu closes when it loses focus.
                    let _ = SetForegroundWindow(self.hwnd);
                    let cmd = TrackPopupMenuEx(
                        popup,
                        (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
                        pt.x,
                        pt.y,
                        self.hwnd,
                        None,
                    );
                    let _ = DestroyMenu(popup);
                    let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                    let id = cmd.0 as usize;
                    if id != 0 {
                        self.run_id(id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Dialog controls: transparent background, palette text, palette brush.
    pub fn on_ctlcolor_any(&self, wp: WPARAM) -> LRESULT {
        let hdc = HDC(wp.0 as *mut core::ffi::c_void);
        unsafe {
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, self.palette.text);
        }
        LRESULT(self.palette.background.0 as isize)
    }

    pub fn on_erase(&self, wp: WPARAM) -> LRESULT {
        let hdc = HDC(wp.0 as *mut core::ffi::c_void);
        let rect = window::client_rect(self.hwnd);
        unsafe {
            FillRect(hdc, &rect, self.palette.background);
        }
        LRESULT(1)
    }

    /// Static labels and buttons: transparent background, palette text.
    pub fn on_ctlcolor(&self, wp: WPARAM) -> LRESULT {
        let hdc = HDC(wp.0 as *mut core::ffi::c_void);
        unsafe {
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, self.palette.text);
        }
        LRESULT(self.palette.background.0 as isize)
    }

    /// Text fields need an opaque background, otherwise the block cursor smears.
    pub fn on_ctlcolor_edit(&self, wp: WPARAM) -> LRESULT {
        let hdc = HDC(wp.0 as *mut core::ffi::c_void);
        unsafe {
            SetBkColor(hdc, COLORREF(self.palette.colors.surface));
            SetTextColor(hdc, self.palette.text);
        }
        LRESULT(self.palette.surface.0 as isize)
    }

    pub fn retheme(&mut self) {
        let colors = theme::palette_for(&self.settings.theme);
        self.palette.dispose();
        self.palette = theme::Palette::new(colors);
        theme::frame(self.hwnd, colors.dark);
        self.apply_theme();
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
        self.layout();
    }

    // ---- shell integration ---------------------------------------------

    fn tray_add(&mut self) -> bool {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: self.small_icon,
            ..Default::default()
        };
        wide::copy_into(&mut data.szTip, &strings::tray_tooltip(None, 0));
        let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &data).as_bool() };
        crate::dbg(&format!(
            "tray: add ok={} icon={:?}",
            ok,
            self.small_icon.0
        ));
        ok
    }

    pub fn update_tray(&mut self) {
        if !self.tray {
            return;
        }
        let (done, total, running) = self.totals();
        let percent = if total > 0 {
            Some((done as f64 * 100.0 / total as f64).round() as u32)
        } else {
            None
        };
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            uFlags: NIF_TIP,
            ..Default::default()
        };
        wide::copy_into(&mut data.szTip, &strings::tray_tooltip(percent, running));
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    fn taskbar_init(&mut self) -> Option<ITaskbarList3> {
        unsafe {
            let bar: ITaskbarList3 =
                CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok()?;
            bar.HrInit().ok()?;
            Some(bar)
        }
    }

    pub fn update_taskbar(&self) {
        let Some(bar) = &self.taskbar else { return };
        let (done, total, running) = self.totals();
        let paused = self.tasks.iter().any(|t| t.status == Status::Paused);
        let failed = self.tasks.iter().any(|t| t.status == Status::Failed);
        unsafe {
            if running == 0 && !paused {
                let _ = bar.SetProgressState(self.hwnd, TBPF_NOPROGRESS);
            } else {
                let state = if failed {
                    TBPF_ERROR
                } else if paused && running == 0 {
                    TBPF_PAUSED
                } else {
                    TBPF_NORMAL
                };
                let _ = bar.SetProgressState(self.hwnd, state);
                let _ = bar.SetProgressValue(self.hwnd, done, total.max(1));
            }
        }
    }

    pub fn totals(&self) -> (u64, u64, usize) {
        let mut done = 0u64;
        let mut total = 0u64;
        let mut running = 0usize;
        for t in &self.tasks {
            if matches!(t.status, Status::Running | Status::Queued) {
                running += 1;
                done += t.done;
                total += t.total;
            }
        }
        (done, total, running)
    }

    pub fn free_space(&self) -> String {
        let dir = wide::wide(&self.settings.dir);
        let mut free: u64 = 0;
        let mut total: u64 = 0;
        let mut avail: u64 = 0;
        unsafe {
            let _ = GetDiskFreeSpaceExW(
                wide::pcw(&dir),
                Some(&mut avail),
                Some(&mut total),
                Some(&mut free),
            );
        }
        let _ = total;
        format::gib_free(free)
    }

    /// Tools ▸ yt-dlp: fetch the official binary into our data folder and
    /// re-scan. Runs on a thread so the UI never freezes on a 30 MB download.
    fn install_ytdlp(&mut self) {
        if let Some(exe) = self.engine.ytdlp_path() {
            let msg = format!("yt-dlp is already available:\n{}", exe.display());
            unsafe {
                MessageBoxW(
                    Some(self.hwnd),
                    wide::pcw(&wide::wide(&msg)),
                    w!("yt-dlp"),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            return;
        }
        let target = paths::data_dir().join("bin").join(tree_core::ytdlp::BINARY);
        // HWND is a pointer: carry it across the thread as an integer.
        let hwnd_raw = self.hwnd.0 as isize;
        unsafe {
            let _ = EnableWindow(self.hwnd, false);
        }
        std::thread::spawn(move || {
            let ok = tree_core::ytdlp::install(&target).is_ok();
            unsafe {
                let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
                let _ = PostMessageW(
                    Some(hwnd),
                    WM_TOOLS_DONE,
                    WPARAM(ok as usize),
                    LPARAM(0),
                );
            }
        });
    }

    pub fn shutdown(&mut self) {
        // Remember where the window was.
        let mut rect = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut rect);
        }
        if rect.right > rect.left && rect.bottom > rect.top {
            self.settings.win_w = (rect.right - rect.left) * 96 / self.dpi.max(96) as i32;
            self.settings.win_h = (rect.bottom - rect.top) * 96 / self.dpi.max(96) as i32;
            self.settings.win_x = rect.left;
            self.settings.win_y = rect.top;
        }
        self.save_settings();

        if self.tray {
            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: TRAY_ID,
                ..Default::default()
            };
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            self.tray = false;
        }
        self.engine.shutdown();
        if !self.font.0.is_null() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
        }
        self.palette.dispose();
    }
}

/// Icon resource id in `res/app.rc`.
const APP_ICON: u16 = 1;

fn load_icon(small: bool) -> HICON {
    unsafe {
        let (cx, cy) = if small {
            (
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
            )
        } else {
            (GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON))
        };
        // MAKEINTRESOURCE(1): the icon lives at id 1 in app.rc. Passing a
        // dangling pointer here silently fell back to the generic system icon,
        // which in the tray looks like no icon at all.
        LoadImageW(
            Some(window::instance()),
            PCWSTR(APP_ICON as usize as *const u16),
            IMAGE_ICON,
            cx,
            cy,
            LR_DEFAULTCOLOR,
        )
        .ok()
        .map(|handle| HICON(handle.0))
        .or_else(|| LoadIconW(None, IDI_APPLICATION).ok())
        .unwrap_or_default()
    }
}

fn cue_banner(edit: HWND, text: &str) {
    let t = wide::wide(text);
    unsafe {
        SendMessageW(
            edit,
            windows::Win32::UI::Controls::EM_SETCUEBANNER,
            Some(WPARAM(1)),
            Some(LPARAM(wide::pcw(&t).0 as isize)),
        );
    }
}

pub unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = crate::APP.load(Ordering::Relaxed);
    if ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    }
    let app = unsafe { &mut *ptr };

    if msg == app.taskbar_created && msg != 0 {
        // Explorer restarted: the tray icon has to be registered again.
        app.tray = app.tray_add();
        app.update_tray();
        return LRESULT(0);
    }

    match msg {
        WM_CREATE => LRESULT(0),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            shell::paint(app, hdc);
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_MEASUREITEM => {
            let mis = unsafe { &mut *(lp.0 as *mut windows::Win32::UI::Controls::MEASUREITEMSTRUCT) };
            if mis.CtlID == controls::ID_LIST as u32 {
                mis.itemHeight = app.s(shell::ROW_H) as u32;
            }
            LRESULT(1)
        }
        WM_MOUSEMOVE => {
            let x = (lp.0 & 0xFFFF) as i16 as i32;
            let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;
            let hot = shell::hit(app, x, y);
            if hot != app.hover {
                app.hover = hot;
                invalidate(hwnd);
            }
            if hot.is_some() {
                app.track_leave();
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            app.tracking = false;
            if app.hover.take().is_some() {
                invalidate(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lp.0 & 0xFFFF) as i16 as i32;
            let y = ((lp.0 >> 16) & 0xFFFF) as i16 as i32;
            if shell::click(app, x, y) {
                invalidate(hwnd);
            }
            LRESULT(0)
        }
        WM_SIZE => {
            app.layout();
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let info = unsafe { &mut *(lp.0 as *mut MINMAXINFO) };
            info.ptMinTrackSize.x = app.s(720);
            info.ptMinTrackSize.y = app.s(420);
            LRESULT(0)
        }
        WM_ERASEBKGND => app.on_erase(wp),
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => app.on_ctlcolor(wp),
        WM_CTLCOLOREDIT => app.on_ctlcolor_edit(wp),
        WM_TIMER => {
            if wp.0 == TIMER_UI {
                app.on_timer();
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wp.0 & 0xFFFF;
            let code = ((wp.0 >> 16) & 0xFFFF) as u32;
            if id == controls::ID_FILTER as usize && code == EN_CHANGE {
                app.refresh();
            } else if code == 0 || code == 1 {
                app.run_id(id);
            }
            LRESULT(0)
        }
        WM_NOTIFY => LRESULT(app.on_notify(lp).unwrap_or(0)),
        WM_DRAWITEM => LRESULT(app.on_draw_item(lp) as isize),
        WM_DROPFILES => {
            app.on_drop(wp);
            LRESULT(0)
        }
        WM_COPYDATA => {
            let urls = unsafe { single::read_copy_data(lp) };
            if !urls.is_empty() {
                app.add_many(&urls);
                unsafe {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            LRESULT(1)
        }
        WM_SETTINGCHANGE => {
            if app.settings.theme == "system" {
                app.retheme();
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            app.dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96);
            let suggested = unsafe { &*(lp.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            app.layout();
            LRESULT(0)
        }
        WM_TRAY => {
            app.on_tray(lp);
            LRESULT(0)
        }
        WM_TOOLS_DONE => {
            unsafe {
                let _ = EnableWindow(hwnd, true);
            }
            app.engine.refresh_tools();
            let (text, icon) = if wp.0 != 0 {
                (
                    "yt-dlp is installed. Video and audio links now download through it.",
                    MB_ICONINFORMATION,
                )
            } else {
                (
                    "yt-dlp could not be downloaded. Check the connection and try again from Tools.",
                    MB_ICONWARNING,
                )
            };
            unsafe {
                MessageBoxW(
                    Some(hwnd),
                    wide::pcw(&wide::wide(text)),
                    w!("yt-dlp"),
                    MB_OK | icon,
                );
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // Closing hides only when there is a tray icon to come back from.
            if app.settings.minimize_to_tray && app.tray {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            } else {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            app.shutdown();
            crate::APP.store(std::ptr::null_mut(), Ordering::Relaxed);
            unsafe {
                KillTimer(Some(hwnd), TIMER_UI).ok();
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}



//! Application state and the window procedure. All user actions funnel through
//! `run_id`, so the toolbar, the menu, the tray and the context menu cannot drift
//! apart.

use crate::paths;
use crate::single;
use crate::ui::{add_dialog, context_menu, format, strings};
use crate::win32::{controls, listview, menu, theme, wide, window};
use std::sync::atomic::Ordering;
use std::time::Instant;
use tree_core::{Engine, Settings, Status, Task};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, FillRect, InvalidateRect, SetBkMode, SetTextColor, HDC, HGDIOBJ, HFONT,
    TRANSPARENT,
};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Controls::{NMHDR, NM_DBLCLK, NM_RCLICK};
use windows::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, ITaskbarList3, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW, ShellExecuteW, Shell_NotifyIconW, TaskbarList, TBPF_ERROR,
    TBPF_NORMAL,
    TBPF_NOPROGRESS, TBPF_PAUSED, HDROP,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::UpdateWindow;

pub const WM_TRAY: u32 = WM_APP + 1;
pub const TIMER_UI: usize = 1;
pub const TRAY_ID: u32 = 1;

pub struct Ui {
    pub list: HWND,
    pub status: HWND,
    pub progress: HWND,
    pub filter: HWND,
    pub buttons: Vec<HWND>,
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
    pub dark: bool,
    pub palette: theme::Palette,
    pub dpi: u32,
    pub hmenu: Option<HMENU>,
    pub tray: bool,
    pub taskbar: Option<ITaskbarList3>,
    pub last_clip: String,
    pub started: Instant,
    pub ticks: u32,
    pub add_dlg: HWND,
}

fn scale(dpi: u32, v: i32) -> i32 {
    (v * dpi as i32 + 48) / 96
}

impl App {
    pub fn new() -> Box<App> {
        let engine = Engine::start(paths::state_path(), paths::downloads_dir());
        let mut settings = engine.settings();
        settings.sane_columns();
        let dark = match settings.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => theme::system_is_dark(),
        };
        let icon = load_icon();
        let hmenu = menu::build(
            settings.conns,
            settings.speed_limit,
            settings.watch_clipboard,
            settings.minimize_to_tray,
            &settings.theme,
        );
        Box::new(App {
            hwnd: HWND::default(),
            engine,
            ui: Ui {
                list: HWND::default(),
                status: HWND::default(),
                progress: HWND::default(),
                filter: HWND::default(),
                buttons: Vec::new(),
            },
            settings,
            tasks: Vec::new(),
            rows: Vec::new(),
            font: theme::message_font(),
            icon,
            dark,
            palette: theme::Palette::for_dark(dark),
            dpi: 96,
            hmenu,
            tray: false,
            taskbar: None,
            last_clip: String::new(),
            started: Instant::now(),
            ticks: 0,
            add_dlg: HWND::default(),
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
            let hwnd = window::create(
                window::MAIN_CLASS,
                strings::WINDOW_TITLE,
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                WS_EX_APPWINDOW,
                None,
                self.s(980),
                self.s(620),
            )?;
            self.hwnd = hwnd;
            self.dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96);
            if let Some(hmenu) = self.hmenu {
                let _ = SetMenu(hwnd, Some(hmenu));
            }
            theme::frame(hwnd, self.dark);
            DragAcceptFiles(hwnd, true);

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
            let width = self
                .settings
                .col_widths
                .get(i)
                .copied()
                .unwrap_or_else(|| self.s(120));
            listview::add_column(list, i as i32, title, self.s(width));
        }
        let status = controls::statusbar(h, controls::ID_STATUS);
        status_parts_init(status);
        let progress = controls::progress(h, controls::ID_PROGRESS, 0, 0, 10, 10);
        controls::progress_range(progress, 1000);
        let filter = controls::edit(h, controls::ID_FILTER, false, 0, 0, 10, 10);
        cue_banner(filter, strings::FILTER_HINT);

        let mut buttons = Vec::new();
        for (id, label) in [
            (controls::ID_ADD, strings::BTN_ADD),
            (controls::ID_PAUSE, strings::BTN_PAUSE),
            (controls::ID_RESUME, strings::BTN_RESUME),
            (controls::ID_REMOVE, strings::BTN_REMOVE),
            (controls::ID_FOLDER, strings::BTN_FOLDER),
        ] {
            buttons.push(controls::button(h, id, label, 0, 0, 10, 10));
        }

        let font = self.font;
        for child in [list, status, progress, filter] {
            controls::set_font(child, font);
        }
        for b in &buttons {
            controls::set_font(*b, font);
        }
        theme::explorer_theme(list, self.dark);

        self.ui = Ui {
            list,
            status,
            progress,
            filter,
            buttons,
        };
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
            _ => {
                if id > menu::ID_CONNS_BASE && id <= menu::ID_CONNS_BASE + 16 {
                    self.settings.conns = id - menu::ID_CONNS_BASE;
                } else if id > menu::ID_SPEED_BASE && id <= menu::ID_SPEED_BASE + 64 {
                    self.settings.speed_limit = ((id - menu::ID_SPEED_BASE) as u64) << 20;
                } else if (menu::ID_THEME_BASE..=menu::ID_THEME_BASE + 2).contains(&id) {
                    self.settings.theme = ["system", "light", "dark"][id - menu::ID_THEME_BASE].into();
                    self.retheme();
                }
                self.save_settings();
            }
        }
        self.refresh();
    }

    fn selected(&self) -> Vec<u64> {
        listview::selected_rows(self.ui.list)
            .into_iter()
            .filter_map(|row| self.rows.get(row as usize).copied())
            .collect()
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
        // Column widths as the user left them.
        let mut widths = Vec::new();
        for i in 0..6 {
            widths.push(self.column_width(i));
        }
        self.settings.col_widths = widths;
        self.engine.set_settings(self.settings.clone());
        if let Some(old) = self.hmenu.take() {
            menu::destroy(old);
        }
        self.hmenu = menu::build(
            self.settings.conns,
            self.settings.speed_limit,
            self.settings.watch_clipboard,
            self.settings.minimize_to_tray,
            &self.settings.theme,
        );
        if let Some(hmenu) = self.hmenu {
            unsafe {
                let _ = SetMenu(self.hwnd, Some(hmenu));
                let _ = DrawMenuBar(self.hwnd);
            }
        }
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

    pub fn on_notify(&mut self, lp: LPARAM) {
        unsafe {
            let hdr = &*(lp.0 as *const NMHDR);
            let code = hdr.code as u32;
            if code == windows::Win32::UI::Controls::LVN_COLUMNCLICK as u32 {
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
            } else if code == NM_DBLCLK {
                self.open_selected();
            } else if code == NM_RCLICK {
                context_menu::show(self);
            }
        }
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
            self.engine.add_many(&urls);
            self.refresh();
        }
    }

    pub fn on_timer(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        self.refresh();
        if self.ticks % 4 == 0 {
            self.poll_clipboard();
        }
        if self.ticks % 8 == 0 {
            self.update_tray();
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
                self.refresh();
            }
        }
    }

    pub fn on_tray(&mut self, lp: LPARAM) {
        let event = (lp.0 as u32) & 0xFFFF;
        if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(self.hwnd);
            }
        } else if event == WM_RBUTTONUP {
            if let Some(menu) = menu::tray_menu() {
                let mut pt = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut pt);
                    let _ = SetForegroundWindow(self.hwnd);
                    let cmd = TrackPopupMenuEx(
                        menu,
                        (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
                        pt.x,
                        pt.y,
                        self.hwnd,
                        None,
                    );
                    let _ = DestroyMenu(menu);
                    match cmd.0 as usize {
                        menu::ID_ADD => {
                            let _ = ShowWindow(self.hwnd, SW_RESTORE);
                            let _ = SetForegroundWindow(self.hwnd);
                        }
                        menu::ID_EXIT => {
                            let _ = DestroyWindow(self.hwnd);
                        }
                        other => self.run_id(other),
                    }
                }
            }
        }
    }

    pub fn on_erase(&self, wp: WPARAM) -> LRESULT {
        let hdc = HDC(wp.0 as *mut core::ffi::c_void);
        let mut rect = window::client_rect(self.hwnd);
        unsafe {
            FillRect(hdc, &mut rect, self.palette.background);
        }
        LRESULT(1)
    }

    pub fn on_ctlcolor(&self, wp: WPARAM) -> LRESULT {
        let hdc = HDC(wp.0 as *mut core::ffi::c_void);
        unsafe {
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, self.palette.text);
        }
        LRESULT(self.palette.background.0 as isize)
    }

    pub fn retheme(&mut self) {
        self.dark = match self.settings.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => theme::system_is_dark(),
        };
        self.palette.dispose();
        self.palette = theme::Palette::for_dark(self.dark);
        theme::frame(self.hwnd, self.dark);
        theme::explorer_theme(self.ui.list, self.dark);
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }

    // ---- shell integration ---------------------------------------------

    fn tray_add(&mut self) -> bool {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon,
            ..Default::default()
        };
        wide::copy_into(&mut data.szTip, "Tree — starting");
        unsafe { Shell_NotifyIconW(NIM_ADD, &data).as_bool() }
    }

    pub fn update_tray(&mut self) {
        if !self.tray {
            return;
        }
        let (done, total, running) = self.totals();
        let label = if running == 0 {
            "Tree — idle".to_string()
        } else if total > 0 {
            format!(
                "Tree — {:.0}% of {} active",
                done as f64 * 100.0 / total as f64,
                running
            )
        } else {
            format!("Tree — {} active", running)
        };
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            uFlags: NIF_TIP,
            ..Default::default()
        };
        wide::copy_into(&mut data.szTip, &label);
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

    pub fn shutdown(&mut self) {
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
        self.save_settings();
        self.engine.shutdown();
        if !self.font.0.is_null() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
        }
        self.palette.dispose();
    }
}

fn load_icon() -> HICON {
    unsafe {
        LoadImageW(
            Some(window::instance()),
            PCWSTR(1 as *const u16),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
        .ok()
        .map(|handle| HICON(handle.0))
        .or_else(|| LoadIconW(None, IDI_APPLICATION).ok())
        .unwrap_or_default()
    }
}

fn status_parts_init(sb: HWND) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(sb, &mut rect);
    }
    let w = rect.right.max(300);
    let parts = [w * 50 / 100, w * 78 / 100, -1];
    controls::status_parts(sb, &parts);
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

pub unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    let ptr = crate::APP.load(Ordering::Relaxed);
    if ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
    }
    let app = unsafe { &mut *ptr };

    match msg {
        WM_CREATE => LRESULT(0),
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
        WM_TIMER => {
            if wp.0 == TIMER_UI {
                app.on_timer();
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wp.0 & 0xFFFF) as usize;
            let code = ((wp.0 >> 16) & 0xFFFF) as u32;
            if id == controls::ID_FILTER as usize && code == EN_CHANGE {
                app.refresh();
            } else if id == controls::ID_BROWSE as usize {
                add_dialog::browse(app);
            } else if code == 0 || code == 1 {
                match id {
                    x if x == controls::ID_OK as usize => add_dialog::commit(app),
                    x if x == controls::ID_CANCEL as usize => add_dialog::close(app),
                    _ => app.run_id(id),
                }
            }
            LRESULT(0)
        }
        WM_NOTIFY => {
            app.on_notify(lp);
            LRESULT(0)
        }
        WM_DROPFILES => {
            app.on_drop(wp);
            LRESULT(0)
        }
        WM_COPYDATA => {
            let urls = unsafe { single::read_copy_data(lp) };
            if !urls.is_empty() {
                app.engine.add_many(&urls);
                app.refresh();
                unsafe {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
            LRESULT(1)
        }
        WM_SETTINGCHANGE => {
            app.retheme();
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
        WM_CLOSE => {
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

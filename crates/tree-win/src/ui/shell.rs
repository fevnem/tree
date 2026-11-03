//! Tree's own chrome: toolbar, sidebar, detail pane and status strip.
//!
//! The table in the middle stays a real `SysListView32` so sorting, keyboard
//! navigation, scrolling and accessibility keep working; everything around it is
//! painted here, because stock controls refuse to follow a dark palette and a
//! half-light window is worse than no dark mode at all.

use crate::app::{App, Hot};
use crate::ui::format;
use crate::win32::{menu, window};
use tree_core::Status;

use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateSolidBrush, DeleteDC,
    DeleteObject, DrawTextW, FillRect, LineTo, MoveToEx, Polygon, Polyline, RoundRect, SelectObject,
    SetBkMode, SetTextColor, HDC, HGDIOBJ, PS_NULL, PS_SOLID, SRCCOPY, TRANSPARENT, DRAW_TEXT_FORMAT,
    DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER,
};
use windows::Win32::UI::Controls::LVM_GETCOLUMNWIDTH;
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
use windows::Win32::Foundation::{LPARAM, WPARAM};

// Logical sizes; every one is scaled through `App::s` at use.
pub const TOOLBAR_H: i32 = 46;
pub const NAV_W: i32 = 190;
pub const DETAIL_H: i32 = 116;
pub const STATUS_H: i32 = 28;
pub const ROW_H: i32 = 44;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    All,
    Downloading,
    Queued,
    Done,
    Failed,
    Paused,
}

impl Filter {
    pub const ALL: [Filter; 6] = [
        Filter::All,
        Filter::Downloading,
        Filter::Queued,
        Filter::Done,
        Filter::Failed,
        Filter::Paused,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All downloads",
            Filter::Downloading => "Downloading",
            Filter::Queued => "Queued",
            Filter::Done => "Completed",
            Filter::Failed => "Failed",
            Filter::Paused => "Paused",
        }
    }

    /// Short label for the status strip.
    pub fn word(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Downloading => "active",
            Filter::Queued => "queued",
            Filter::Done => "completed",
            Filter::Failed => "failed",
            Filter::Paused => "paused",
        }
    }

    pub fn matches(self, status: Status) -> bool {
        match self {
            Filter::All => true,
            Filter::Downloading => status == Status::Running,
            Filter::Queued => status == Status::Queued,
            Filter::Done => status == Status::Done,
            Filter::Failed => status == Status::Failed,
            Filter::Paused => status == Status::Paused,
        }
    }

    pub fn index(self) -> usize {
        Filter::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glyph {
    Plus,
    Pause,
    Play,
    Trash,
    Folder,
    Arrow,
}

pub struct Tool {
    pub id: usize,
    pub glyph: Glyph,
    pub label: &'static str,
}

impl Tool {
    /// A tool is only usable when the selection makes it meaningful.
    pub fn enabled(&self, app: &App) -> bool {
        let selected = app.selected_task().is_some();
        match self.id {
            menu::ID_ADD => true,
            menu::ID_FOLDER | menu::ID_REMOVE => selected,
            menu::ID_OPEN => app.selected_task().is_some_and(|t| t.status == Status::Done),
            menu::ID_PAUSE => app
                .selected_task()
                .is_some_and(|t| matches!(t.status, Status::Running | Status::Queued)),
            menu::ID_RESUME => app
                .selected_task()
                .is_some_and(|t| matches!(t.status, Status::Paused | Status::Failed)),
            _ => selected,
        }
    }
}

pub const TOOLS: [Tool; 6] = [
    Tool { id: menu::ID_ADD, glyph: Glyph::Plus, label: "Add" },
    Tool { id: menu::ID_PAUSE, glyph: Glyph::Pause, label: "Pause" },
    Tool { id: menu::ID_RESUME, glyph: Glyph::Play, label: "Resume" },
    Tool { id: menu::ID_REMOVE, glyph: Glyph::Trash, label: "Remove" },
    Tool { id: menu::ID_FOLDER, glyph: Glyph::Folder, label: "Folder" },
    Tool { id: menu::ID_OPEN, glyph: Glyph::Arrow, label: "Open" },
];

pub struct Layout {
    pub toolbar: RECT,
    pub nav: RECT,
    pub list: RECT,
    pub details: RECT,
    pub status: RECT,
}

pub fn layout(app: &App) -> Layout {
    let client = window::client_rect(app.hwnd);
    let w = (client.right - client.left).max(1);
    let h = (client.bottom - client.top).max(1);
    let tb = app.s(TOOLBAR_H).min(h / 3);
    let st = app.s(STATUS_H);
    let nav_w = app.s(NAV_W).min(w / 3);
    // Drop the detail pane before it starts squeezing the list.
    let det = if h - tb - st > app.s(320) { app.s(DETAIL_H) } else { 0 };
    let mid_right = w;
    Layout {
        toolbar: RECT { left: 0, top: 0, right: w, bottom: tb },
        nav: RECT { left: 0, top: tb, right: nav_w, bottom: h - st },
        list: RECT { left: nav_w, top: tb, right: mid_right, bottom: h - st - det },
        details: RECT { left: nav_w, top: h - st - det, right: mid_right, bottom: h - st },
        status: RECT { left: 0, top: h - st, right: w, bottom: h },
    }
}

fn tool_rect(app: &App, l: &Layout, index: usize) -> RECT {
    let pad = app.s(10);
    let width = app.s(96);
    let height = app.s(30);
    let gap = app.s(4);
    let x = l.toolbar.left + pad + index as i32 * (width + gap);
    let top = l.toolbar.top + (l.toolbar.bottom - l.toolbar.top - height) / 2;
    RECT { left: x, top, right: x + width, bottom: top + height }
}

fn nav_rect(app: &App, l: &Layout, index: usize) -> RECT {
    let top = l.nav.top + app.s(34) + index as i32 * app.s(30);
    RECT {
        left: l.nav.left + app.s(8),
        top,
        right: l.nav.right - app.s(8),
        bottom: top + app.s(28),
    }
}

pub fn hit(app: &App, x: i32, y: i32) -> Option<Hot> {
    let l = layout(app);
    let inside = |r: &RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    for i in 0..TOOLS.len() {
        if inside(&tool_rect(app, &l, i)) {
            return Some(Hot::Tool(i));
        }
    }
    for i in 0..Filter::ALL.len() {
        if inside(&nav_rect(app, &l, i)) {
            return Some(Hot::Nav(i));
        }
    }
    if app.tasks.is_empty() && inside(&empty_button(app, &l)) {
        return Some(Hot::Empty);
    }
    None
}

fn empty_button(app: &App, l: &Layout) -> RECT {
    let w = app.s(160);
    let h = app.s(34);
    let cx = (l.list.left + l.list.right) / 2;
    let cy = (l.list.top + l.list.bottom) / 2 + app.s(26);
    RECT { left: cx - w / 2, top: cy, right: cx + w / 2, bottom: cy + h }
}

pub fn click(app: &mut App, x: i32, y: i32) -> bool {
    match hit(app, x, y) {
        Some(Hot::Tool(i)) => {
            app.run_id(TOOLS[i].id);
            true
        }
        Some(Hot::Nav(i)) => {
            app.set_filter(Filter::ALL[i]);
            true
        }
        Some(Hot::Empty) => {
            app.run_id(menu::ID_ADD);
            true
        }
        None => false,
    }
}

// ---------------------------------------------------------------- painting

fn rect(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

fn fill(hdc: HDC, r: &RECT, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color));
        FillRect(hdc, r, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// Round-rect fill with no outline (RoundRect draws its border with the pen).
fn rounded(hdc: HDC, r: RECT, radius: i32, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color));
        let pen = CreatePen(PS_NULL, 0, COLORREF(0));
        let old_b = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_p = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

fn line(hdc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, width: i32, color: u32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, width, COLORREF(color));
        let old = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = MoveToEx(hdc, x1, y1, None);
        let _ = LineTo(hdc, x2, y2);
        SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

fn triangle(hdc: HDC, points: &[windows::Win32::Foundation::POINT], color: u32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color));
        let pen = CreatePen(PS_NULL, 0, COLORREF(0));
        let old_b = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_p = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = Polygon(hdc, points);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

fn text(hdc: HDC, s: &str, r: RECT, color: u32, flags: DRAW_TEXT_FORMAT, font: HGDIOBJ) {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(color));
        let mut rc = r;
        let _ = DrawTextW(hdc, &mut wide, &mut rc, flags);
        SelectObject(hdc, old);
    }
}

fn text_flags(align: DRAW_TEXT_FORMAT) -> DRAW_TEXT_FORMAT {
    DRAW_TEXT_FORMAT(
        align.0 | DT_SINGLELINE.0 | DT_VCENTER.0 | DT_END_ELLIPSIS.0 | DT_NOPREFIX.0,
    )
}

/// COLORREF is 0x00BBGGRR, so every literal goes through here.
pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    (b << 16) | (g << 8) | r
}

pub const GREEN: u32 = rgb(74, 222, 128);
pub const RED: u32 = rgb(248, 113, 113);
pub const AMBER: u32 = rgb(251, 191, 36);

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let f = |shift: u32| {
        let x = ((a >> shift) & 0xFF) as f32;
        let y = ((b >> shift) & 0xFF) as f32;
        ((x + (y - x) * t).round() as u32) & 0xFF
    };
    f(0) | (f(8) << 8) | (f(16) << 16)
}

fn status_color(app: &App, status: Status) -> u32 {
    let c = app.palette.colors;
    match status {
        Status::Running => c.accent,
        Status::Done => GREEN,
        Status::Failed => RED,
        Status::Paused => AMBER,
        Status::Queued => c.dim,
    }
}

pub fn paint(app: &App, hdc: HDC) {
    let client = window::client_rect(app.hwnd);
    let w = (client.right - client.left).max(1);
    let h = (client.bottom - client.top).max(1);
    let l = layout(app);
    let colors = app.palette.colors;

    unsafe {
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));

        fill(mem, &rect(0, 0, w, h), colors.bg);
        paint_toolbar(app, mem, &l);
        paint_nav(app, mem, &l);
        if l.details.bottom > l.details.top {
            paint_details(app, mem, &l);
        }
        paint_status(app, mem, &l);
        if app.tasks.is_empty() {
            paint_empty(app, mem, &l);
        }

        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
    }
    let _ = HWND::default;
}

fn paint_toolbar(app: &App, hdc: HDC, l: &Layout) {
    let c = app.palette.colors;
    fill(hdc, &l.toolbar, c.bg);
    line(hdc, 0, l.toolbar.bottom - 1, l.toolbar.right, l.toolbar.bottom - 1, 1, mix(c.bg, c.text, 0.10));

    for i in 0..TOOLS.len() {
        let r = tool_rect(app, l, i);
        let tool = &TOOLS[i];
        let enabled = tool.enabled(app);
        let hot = app.hover == Some(Hot::Tool(i)) && enabled;
        if hot {
            rounded(hdc, r_to_rect(r), app.s(8), mix(c.bg, c.text, 0.10));
        }
        let color = if !enabled {
            mix(c.bg, c.text, 0.78)
        } else if hot {
            c.text
        } else {
            mix(c.text, c.bg, 0.10)
        };
        let icon_box = RECT { left: r.left + app.s(8), top: r.top, right: r.left + app.s(36), bottom: r.bottom };
        glyph(app, hdc, tool.glyph, icon_box, color);
        let label = RECT { left: r.left + app.s(38), top: r.top, right: r.right - app.s(4), bottom: r.bottom };
        text(hdc, tool.label, label, color, text_flags(DT_LEFT), HGDIOBJ(app.font.0));
    }
}

fn glyph(app: &App, hdc: HDC, g: Glyph, r: RECT, color: u32) {
    let cx = (r.left + r.right) / 2;
    let cy = (r.top + r.bottom) / 2;
    let s = app.s(1);
    let arm = 7 * s;
    let thick = (2 * s).max(1);
    match g {
        Glyph::Plus => {
            line(hdc, cx - arm, cy, cx + arm, cy, thick, color);
            line(hdc, cx, cy - arm, cx, cy + arm, thick, color);
        }
        Glyph::Pause => {
            fill(hdc, &rect(cx - arm + s, cy - arm, cx - arm + s + 3 * s, cy + arm), color);
            fill(hdc, &rect(cx + arm - 4 * s, cy - arm, cx + arm - s, cy + arm), color);
        }
        Glyph::Play => {
            let pts = [
                windows::Win32::Foundation::POINT { x: cx - arm + s, y: cy - arm },
                windows::Win32::Foundation::POINT { x: cx - arm + s, y: cy + arm },
                windows::Win32::Foundation::POINT { x: cx + arm, y: cy },
            ];
            triangle(hdc, &pts, color);
        }
        Glyph::Trash => {
            line(hdc, cx - arm, cy - arm, cx + arm, cy - arm, thick, color);
            line(hdc, cx - arm / 2, cy - arm - 3 * s, cx + arm / 2, cy - arm - 3 * s, thick, color);
            let body = rect(cx - arm + 2 * s, cy - arm + thick, cx + arm - 2 * s, cy + arm);
            unsafe {
                let brush = CreateSolidBrush(COLORREF(color));
                let old = SelectObject(hdc, HGDIOBJ(brush.0));
                let _ = windows::Win32::Graphics::Gdi::FrameRect(hdc, &body, brush);
                SelectObject(hdc, old);
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
        }
        Glyph::Folder => {
            let tab = rect(cx - arm, cy - arm + 2 * s, cx - arm / 3, cy - arm + 4 * s);
            fill(hdc, &tab, color);
            let body = rect(cx - arm, cy - arm + 4 * s, cx + arm, cy + arm);
            fill(hdc, &body, color);
            let cut = rect(cx - arm + thick, cy - arm + 4 * s + thick, cx + arm - thick, cy + arm - thick);
            fill(hdc, &cut, app.palette.colors.bg);
        }
        Glyph::Arrow => {
            line(hdc, cx - arm, cy + arm, cx + arm, cy - arm, thick, color);
            let head = [
                windows::Win32::Foundation::POINT { x: cx + arm - 5 * s, y: cy - arm },
                windows::Win32::Foundation::POINT { x: cx + arm, y: cy - arm },
                windows::Win32::Foundation::POINT { x: cx + arm, y: cy - arm + 5 * s },
            ];
            triangle(hdc, &head, color);
        }
    }
}

fn paint_nav(app: &App, hdc: HDC, l: &Layout) {
    let c = app.palette.colors;
    fill(hdc, &l.nav, mix(c.bg, c.text, 0.04));
    line(hdc, l.nav.right - 1, l.nav.top, l.nav.right - 1, l.nav.bottom, 1, mix(c.bg, c.text, 0.10));

    let title = rect(l.nav.left + app.s(16), l.nav.top + app.s(10), l.nav.right, l.nav.top + app.s(30));
    text(hdc, "DOWNLOADS", title, c.dim, text_flags(DT_LEFT), HGDIOBJ(app.font.0));

    for i in 0..Filter::ALL.len() {
        let f = Filter::ALL[i];
        let r = nav_rect(app, l, i);
        if app.filter == f {
            rounded(hdc, r, app.s(7), mix(c.bg, c.accent, 0.20));
            let bar = rect(r.left, r.top + app.s(6), r.left + app.s(3), r.bottom - app.s(6));
            rounded(hdc, bar, app.s(2), c.accent);
        } else if app.hover == Some(Hot::Nav(i)) {
            rounded(hdc, r, app.s(7), mix(c.bg, c.text, 0.06));
        }
        let color = if app.filter == f { c.text } else { mix(c.text, c.bg, 0.25) };
        let label = rect(r.left + app.s(14), r.top, r.right - app.s(40), r.bottom);
        text(hdc, f.label(), label, color, text_flags(DT_LEFT), HGDIOBJ(app.font.0));

        let count = app.counts[i];
        let num = rect(r.right - app.s(38), r.top, r.right - app.s(12), r.bottom);
        let count_color = if app.filter == f {
            c.accent
        } else if count == 0 {
            mix(c.dim, c.bg, 0.45)
        } else {
            c.dim
        };
        text(hdc, &count.to_string(), num, count_color, text_flags(DT_RIGHT), HGDIOBJ(app.font.0));
    }
}

fn paint_details(app: &App, hdc: HDC, l: &Layout) {
    let c = app.palette.colors;
    fill(hdc, &l.details, c.bg);
    line(hdc, l.details.left, l.details.top, l.details.right, l.details.top, 1, mix(c.bg, c.text, 0.10));

    let pad = app.s(14);
    let Some(task) = app.selected_task() else {
        let r = rect(l.details.left + pad, l.details.top, l.details.right, l.details.bottom);
        text(
            hdc,
            "Select a download to see its connections.",
            r,
            c.dim,
            text_flags(DT_LEFT),
            HGDIOBJ(app.font.0),
        );
        return;
    };

    let left_w = (l.details.right - l.details.left) * 55 / 100;
    let name_r = rect(l.details.left + pad, l.details.top + app.s(8), l.details.left + left_w, l.details.top + app.s(30));
    text(hdc, &task.name, name_r, c.text, text_flags(DT_LEFT), HGDIOBJ(app.font.0));

    let dir_r = rect(l.details.left + pad, l.details.top + app.s(30), l.details.left + left_w, l.details.top + app.s(50));
    text(hdc, &format!("{} · {}", format::size(task.total), task.status.word()), dir_r, c.dim, text_flags(DT_LEFT), HGDIOBJ(app.font.0));

    let url_r = rect(l.details.left + pad, l.details.top + app.s(50), l.details.left + left_w, l.details.top + app.s(70));
    text(hdc, &task.url, url_r, c.dim, text_flags(DT_LEFT), HGDIOBJ(app.font.0));

    if let Some(err) = &task.error {
        let err_r = rect(l.details.left + pad, l.details.top + app.s(70), l.details.left + left_w, l.details.top + app.s(90));
        text(hdc, err, err_r, RED, text_flags(DT_LEFT), HGDIOBJ(app.font.0));
    }

    // Segment grid: one cell per connection, filled by its own progress.
    let grid_left = l.details.left + left_w + app.s(8);
    let grid_right = l.details.right - pad;
    let n = task.segments.len().max(1);
    let gap = app.s(3);
    let cell_w = ((grid_right - grid_left) - gap * (n as i32 - 1)) / n as i32;
    if cell_w < app.s(4) {
        return;
    }
    let top = l.details.top + app.s(14);
    let height = app.s(14);
    let caption = rect(grid_left, top, grid_right, top + app.s(16));
    text(
        hdc,
        &format!("{} CONNECTION{}", n, if n == 1 { "" } else { "S" }),
        caption,
        c.dim,
        text_flags(DT_LEFT),
        HGDIOBJ(app.font.0),
    );

    let fill_color = status_color(app, task.status);
    for (i, seg) in task.segments.iter().enumerate() {
        let x = grid_left + i as i32 * (cell_w + gap);
        let track = rect(x, top + app.s(18), x + cell_w, top + app.s(18) + height);
        rounded(hdc, track, app.s(3), mix(c.bg, c.text, 0.12));
        let len = (seg.end + 1).saturating_sub(seg.start).max(1);
        let frac = (seg.done as f64 / len as f64).clamp(0.0, 1.0);
        if frac > 0.0 {
            let filled = ((frac * cell_w as f64).round() as i32).clamp(1, cell_w);
            let bar = rect(x, track.top, x + filled, track.bottom);
            let shade = if frac >= 0.999 {
                fill_color
            } else {
                mix(c.bg, fill_color, 0.62)
            };
            rounded(hdc, bar, app.s(3), shade);
        }
        // A segment that finished its share while the task failed is the one to blame.
        if frac >= 0.999 {
            line(hdc, x, track.bottom + app.s(2), x + cell_w, track.bottom + app.s(2), 1, mix(c.bg, fill_color, 0.55));
        }
    }

    // Aggregate bar under the cells.
    let total = task.total.max(1);
    let done = task.done.min(total);
    let track = rect(grid_left, top + app.s(38), grid_right, top + app.s(38) + app.s(6));
    rounded(hdc, track, app.s(3), mix(c.bg, c.text, 0.10));
    let filled = ((done as f64 / total as f64) * (grid_right - grid_left) as f64).round() as i32;
    if filled > 0 {
        let bar = rect(grid_left, track.top, grid_left + filled.max(1), track.bottom);
        rounded(hdc, bar, app.s(3), fill_color);
    }
    let percent = (done as f64 * 100.0 / total as f64).round() as u32;
    let pct_r = rect(grid_left, top + app.s(46), grid_right, top + app.s(66));
    let detail = if task.speed > 1.0 {
        format!("{}% · {}", percent, format::speed(task.speed))
    } else {
        format!("{}%", percent)
    };
    text(hdc, &detail, pct_r, c.dim, text_flags(DT_LEFT), HGDIOBJ(app.font.0));
}

fn paint_status(app: &App, hdc: HDC, l: &Layout) {
    let c = app.palette.colors;
    fill(hdc, &l.status, mix(c.bg, c.text, 0.04));
    line(hdc, l.status.left, l.status.top, l.status.right, l.status.top, 1, mix(c.bg, c.text, 0.10));

    let left = rect(l.status.left + app.s(14), l.status.top, l.status.left + app.s(220), l.status.bottom);
    let speed: f64 = app.tasks.iter().filter(|t| t.status == Status::Running).map(|t| t.speed).sum();
    let text_val = if speed > 1.0 {
        format!("{}  ·  {} active", format::speed(speed), app.counts[1])
    } else {
        format!("Idle  ·  {} active", app.counts[1])
    };
    text(hdc, &text_val, left, if speed > 1.0 { c.accent } else { c.dim }, text_flags(DT_LEFT), HGDIOBJ(app.font.0));

    let mid = rect(l.status.left + app.s(230), l.status.top, l.status.left + app.s(560), l.status.bottom);
    let summary = format!(
        "{} queued  ·  {} completed  ·  {} paused  ·  {} failed",
        app.counts[2], app.counts[3], app.counts[5], app.counts[4]
    );
    text(hdc, &summary, mid, c.dim, text_flags(DT_LEFT), HGDIOBJ(app.font.0));

    // Sparkline of total throughput, newest on the right.
    let graph_w = app.s(180);
    let graph_h = app.s(16);
    let right = l.status.right - app.s(14);
    let left_x = right - graph_w;
    let bottom = l.status.bottom - app.s(7);
    line(hdc, left_x, bottom, right, bottom, 1, mix(c.bg, c.text, 0.16));
    let caption = rect(left_x - app.s(56), l.status.top, left_x - app.s(6), l.status.bottom);
    text(hdc, "last 60s", caption, mix(c.dim, c.bg, 0.25), text_flags(DT_RIGHT), HGDIOBJ(app.font.0));
    let peak_raw = app.speed_hist.iter().copied().max().unwrap_or(0);
    if app.speed_hist.len() > 1 && peak_raw > 0 {
        // Slice against the window's peak so the shape reads even when the
        // rate is steady.
        let peak = peak_raw as f64 * 1.1;
        let count = app.speed_hist.len();
        let step = graph_w as f64 / (count.max(2) - 1) as f64;
        let mut pts: Vec<windows::Win32::Foundation::POINT> = app
            .speed_hist
            .iter()
            .enumerate()
            .map(|(i, v)| windows::Win32::Foundation::POINT {
                x: left_x + (i as f64 * step).round() as i32,
                y: bottom - ((*v as f64 / peak) * graph_h as f64).round() as i32,
            })
            .collect();
        // Close the polygon down to the baseline and fill it: a bare line at
        // full height just looks like a border.
        let mut area = pts.clone();
        area.push(windows::Win32::Foundation::POINT { x: right, y: bottom });
        area.push(windows::Win32::Foundation::POINT { x: left_x, y: bottom });
        unsafe {
            let brush = CreateSolidBrush(COLORREF(mix(c.bg, c.accent, 0.45)));
            let pen = CreatePen(PS_NULL, 0, COLORREF(0));
            let ob = SelectObject(hdc, HGDIOBJ(brush.0));
            let op = SelectObject(hdc, HGDIOBJ(pen.0));
            let _ = Polygon(hdc, &area);
            SelectObject(hdc, ob);
            SelectObject(hdc, op);
            let _ = DeleteObject(HGDIOBJ(brush.0));
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }
        pts.pop();
        unsafe {
            let pen = CreatePen(PS_SOLID, 1, COLORREF(c.accent));
            let old = SelectObject(hdc, HGDIOBJ(pen.0));
            let _ = Polyline(hdc, &pts);
            SelectObject(hdc, old);
            let _ = DeleteObject(HGDIOBJ(pen.0));
        }
    }
}

fn paint_empty(app: &App, hdc: HDC, l: &Layout) {
    let c = app.palette.colors;
    fill(hdc, &l.list, c.bg);
    let cy = (l.list.top + l.list.bottom) / 2;

    let title = rect(l.list.left, cy - app.s(40), l.list.right, cy - app.s(16));
    text(hdc, "Nothing downloading yet", title, c.text, text_flags(DT_LEFT) | DT_CENTER, HGDIOBJ(app.font.0));

    let hint = rect(l.list.left, cy - app.s(14), l.list.right, cy + app.s(8));
    text(
        hdc,
        "Paste a link, drop a file here, or press Ctrl+N.",
        hint,
        c.dim,
        text_flags(DT_LEFT),
        HGDIOBJ(app.font.0),
    );

    let b = empty_button(app, l);
    rounded(hdc, b, app.s(8), if app.hover == Some(Hot::Empty) { mix(c.accent, 0x00FF_FFFF, 0.12) } else { c.accent });
    text(hdc, "Add a download", b, 0x00FF_FFFF, text_flags(DT_LEFT) | DT_CENTER, HGDIOBJ(app.font.0));
}

// ---------------------------------------------------------------- rows

fn column_edges(app: &App) -> Vec<i32> {
    let mut edges = vec![0];
    let mut x = 0;
    for i in 0..6 {
        let w = unsafe {
            SendMessageW(
                app.ui.list,
                LVM_GETCOLUMNWIDTH,
                Some(WPARAM(i)),
                Some(LPARAM(0)),
            )
            .0 as i32
        };
        x += w.max(0);
        edges.push(x);
    }
    edges
}

/// One table row. Called from `WM_DRAWITEM` because the list is
/// `LVS_OWNERDRAWFIXED`: we get the whole row rectangle and paint every cell.
pub fn draw_row(app: &App, hdc: HDC, rc: RECT, row: usize, selected: bool) {
    let c = app.palette.colors;
    let Some(task) = app.tasks.get(row) else { return };
    let edges = column_edges(app);
    let height = rc.bottom - rc.top;

    if selected {
        fill(hdc, &rc, mix(c.bg, c.accent, 0.14));
        let bar = rect(rc.left, rc.top, rc.left + app.s(3), rc.bottom);
        fill(hdc, &bar, c.accent);
    } else if row % 2 == 1 {
        fill(hdc, &rc, mix(c.bg, c.text, 0.03));
    } else {
        fill(hdc, &rc, c.bg);
    }

    let pad = app.s(10);
    let font = HGDIOBJ(app.font.0);

    // Name cell: a file-type badge, then the name.
    let name_r = rect(rc.left + edges[0] + pad, rc.top, rc.left + edges[1] - app.s(6), rc.bottom);
    let badge_w = app.s(40);
    let badge = rect(name_r.left, rc.top + (height - app.s(22)) / 2, name_r.left + badge_w, rc.top + (height + app.s(22)) / 2);
    let (badge_bg, ext) = badge_of(task.name.as_str());
    rounded(hdc, badge, app.s(5), badge_bg);
    text(hdc, &ext, badge, 0x00FF_FFFF, text_flags(DT_LEFT) | DT_CENTER, font);

    let title = rect(badge.right + pad, rc.top, name_r.right, rc.bottom);
    text(hdc, &task.name, title, c.text, text_flags(DT_LEFT), font);

    // Size.
    let size_r = rect(rc.left + edges[1] + pad, rc.top, rc.left + edges[2] - pad, rc.bottom);
    text(hdc, &format::size(task.total), size_r, c.dim, text_flags(DT_RIGHT), font);

    // Progress: bar plus percent.
    let prog_r = rect(rc.left + edges[2] + pad, rc.top, rc.left + edges[3] - pad, rc.bottom);
    let total = task.total.max(1);
    let done = task.done.min(total);
    let frac = done as f64 / total as f64;
    let bar_h = app.s(10);
    let bar = rect(
        prog_r.left,
        rc.top + (height - bar_h) / 2,
        (prog_r.right - app.s(48)).max(prog_r.left + app.s(10)),
        rc.top + (height + bar_h) / 2,
    );
    rounded(hdc, bar, bar_h / 2, mix(c.bg, c.text, 0.13));
    let filled = (frac * (bar.right - bar.left) as f64).round() as i32;
    if filled > 0 {
        let done_r = rect(bar.left, bar.top, bar.left + filled.max(app.s(2)), bar.bottom);
        rounded(hdc, done_r, bar_h / 2, status_color(app, task.status));
    }
    let pct = rect(bar.right + app.s(8), rc.top, prog_r.right, rc.bottom);
    let label = if task.status == Status::Done {
        "100%".to_string()
    } else {
        format!("{}%", (frac * 100.0).round() as u32)
    };
    text(hdc, &label, pct, c.dim, text_flags(DT_RIGHT), font);

    // Speed.
    let speed_r = rect(rc.left + edges[3] + pad, rc.top, rc.left + edges[4] - pad, rc.bottom);
    let speed = if task.speed > 1.0 && task.status == Status::Running {
        format::speed(task.speed)
    } else {
        "—".to_string()
    };
    text(hdc, &speed, speed_r, if task.speed > 1.0 { c.text } else { c.dim }, text_flags(DT_RIGHT), font);

    // ETA.
    let eta_r = rect(rc.left + edges[4] + pad, rc.top, rc.left + edges[5] - pad, rc.bottom);
    let eta = if task.status == Status::Running && task.speed > 1.0 && task.total > 0 {
        let left = task.total.saturating_sub(task.done) as f64;
        format::eta(Some((left / task.speed).max(0.0) as u64))
    } else {
        "—".to_string()
    };
    text(hdc, &eta, eta_r, c.dim, text_flags(DT_RIGHT), font);

    // Status.
    let st_r = rect(rc.left + edges[5] + pad, rc.top, rc.right - pad, rc.bottom);
    text(hdc, task.status.word(), st_r, status_color(app, task.status), text_flags(DT_LEFT), font);

    line(hdc, rc.left, rc.bottom - 1, rc.right, rc.bottom - 1, 1, mix(c.bg, c.text, 0.06));
}

/// With owner-draw the control still paints the area below the last row with its
/// own brush, which under a dark theme comes out pure black. Fill it ourselves.
pub fn paint_list_gap(app: &App, hdc: HDC) {
    let list = app.ui.list;
    let client = window::client_rect(list);
    let count = unsafe {
        SendMessageW(
            list,
            windows::Win32::UI::Controls::LVM_GETITEMCOUNT,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        )
        .0 as i32
    };
    let mut top = client.top;
    if count > 0 {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let ok = unsafe {
            SendMessageW(
                list,
                windows::Win32::UI::Controls::LVM_GETITEMRECT,
                Some(WPARAM((count - 1) as usize)),
                Some(LPARAM(&mut r as *mut RECT as isize)),
            )
        };
        if ok.0 != 0 {
            top = r.bottom;
        }
    }
    let gap = rect(client.left, top, client.right, client.bottom);
    if gap.bottom > gap.top {
        fill(hdc, &gap, app.palette.colors.bg);
    }
}

/// The column header, painted here because `SetWindowTheme` does not reach it.
pub fn paint_header(app: &App, hdc: HDC) {
    let header = app.header_hwnd();
    let rc = window::client_rect(header);
    let c = app.palette.colors;
    fill(hdc, &rc, mix(c.bg, c.text, 0.07));
    let edges = column_edges(app);
    let pad = app.s(10);
    for (i, title) in crate::ui::strings::COLUMNS.iter().enumerate() {
        let cell = rect(rc.left + edges[i], rc.top, rc.left + edges[i + 1], rc.bottom);
        let label = rect(cell.left + pad, cell.top, cell.right - app.s(18), cell.bottom);
        text(hdc, title, label, if app.settings.sort_col == i as i32 { c.text } else { c.dim }, text_flags(DT_LEFT), HGDIOBJ(app.font.0));
        if app.settings.sort_col == i as i32 {
            let arrow_up = !app.settings.sort_desc;
            let cx = cell.right - app.s(12);
            let cy = (cell.top + cell.bottom) / 2;
            let d = app.s(4);
            let pts = if arrow_up {
                [
                    windows::Win32::Foundation::POINT { x: cx - d, y: cy + d / 2 },
                    windows::Win32::Foundation::POINT { x: cx + d, y: cy + d / 2 },
                    windows::Win32::Foundation::POINT { x: cx, y: cy - d },
                ]
            } else {
                [
                    windows::Win32::Foundation::POINT { x: cx - d, y: cy - d / 2 },
                    windows::Win32::Foundation::POINT { x: cx + d, y: cy - d / 2 },
                    windows::Win32::Foundation::POINT { x: cx, y: cy + d },
                ]
            };
            triangle(hdc, &pts, c.accent);
        }
        line(hdc, cell.right - 1, cell.top + app.s(6), cell.right - 1, cell.bottom - app.s(6), 1, mix(c.bg, c.text, 0.06));
    }
    line(hdc, rc.left, rc.bottom - 1, rc.right, rc.bottom - 1, 1, mix(c.bg, c.text, 0.12));
}

fn r_to_rect(r: RECT) -> RECT {
    r
}

/// Badge colour and short label by extension, the way IDM/FDM group files.
fn badge_of(name: &str) -> (u32, String) {
    let ext = name
        .rsplit('.')
        .next()
        .filter(|e| e.len() <= 4 && *e != name)
        .unwrap_or("FILE")
        .to_ascii_uppercase();
    let color = match ext.as_str() {
        "ISO" | "IMG" | "DMG" => rgb(180, 92, 32),
        "ZIP" | "RAR" | "7Z" | "TAR" | "GZ" | "XZ" => rgb(150, 110, 30),
        "MP4" | "MKV" | "AVI" | "MOV" | "WEBM" | "MP3" | "FLAC" | "M4A" => rgb(170, 58, 120),
        "EXE" | "MSI" | "BAT" | "PS1" => rgb(96, 62, 178),
        "PDF" | "DOC" | "DOCX" | "TXT" | "EPUB" => rgb(70, 100, 160),
        "PNG" | "JPG" | "JPEG" | "GIF" | "WEBP" | "SVG" => rgb(38, 130, 110),
        _ => rgb(86, 92, 104),
    };
    (color, ext.chars().take(4).collect())
}

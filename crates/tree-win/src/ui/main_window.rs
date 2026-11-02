//! Layout and the periodic refresh: snapshot → rows → chrome.
//!
//! The ListView is only rebuilt when the visible set changes; otherwise cells
//! are updated in place, which keeps an idle Tree at ~0 % CPU.

use super::format;
use super::strings;
use crate::app::App;
use crate::win32::{controls, listview, window};
use std::cmp::Ordering;
use tree_core::{Status, Task};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_SIZE};

impl App {
    pub fn layout(&mut self) {
        let rect = window::client_rect(self.hwnd);
        let width = (rect.right - rect.left).max(1);
        let height = (rect.bottom - rect.top).max(1);
        let pad = self.s(10);
        let gap = self.s(6);
        let btn_h = self.s(30);
        let bar_h = self.s(16);

        let mut x = pad;
        let bw = self.s(92);
        for button in &self.ui.buttons {
            window::place(*button, x, pad, bw, btn_h);
            x += bw + gap;
        }

        let filter_w = self.s(220);
        let filter_h = self.s(24);
        window::place(
            self.ui.filter,
            width - pad - filter_w,
            pad + (btn_h - filter_h) / 2,
            filter_w,
            filter_h,
        );

        // Status bar positions itself from the parent size.
        unsafe {
            SendMessageW(
                self.ui.status,
                WM_SIZE,
                Some(windows::Win32::Foundation::WPARAM(0)),
                Some(LPARAM(0)),
            );
        }
        let mut sb = windows::Win32::Foundation::RECT::default();
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(
                self.ui.status,
                &mut sb,
            );
        }
        let sb_h = (sb.bottom - sb.top).max(self.s(22));

        let progress_y = height - sb_h - bar_h - self.s(8);
        window::place(self.ui.progress, pad, progress_y, width - pad * 2, bar_h);

        let list_y = pad + btn_h + self.s(10);
        let list_h = (progress_y - self.s(8) - list_y).max(self.s(80));
        window::place(self.ui.list, pad, list_y, width - pad * 2, list_h);

        let parts = [width * 42 / 100, width * 78 / 100, -1];
        controls::status_parts(self.ui.status, &parts);

        // Fill the empty space to the right of ETA by widening the Name column,
        // instead of leaving a phantom seventh column.
        let others: i32 = (1..6)
            .map(|i| unsafe {
                SendMessageW(
                    self.ui.list,
                    windows::Win32::UI::Controls::LVM_GETCOLUMNWIDTH,
                    Some(windows::Win32::Foundation::WPARAM(i as usize)),
                    Some(LPARAM(0)),
                )
                .0 as i32
            })
            .sum();
        let name_width = (width - pad * 2 - others - self.s(4)).max(self.s(220));
        listview::set_column_width(self.ui.list, 0, name_width);
    }

    pub fn refresh(&mut self) {
        let was_empty = self.tasks.is_empty();
        let mut tasks: Vec<Task> = self.engine.snapshot();
        let needle = crate::win32::wide::get_text(self.ui.filter).trim()
        .to_lowercase();
        if !needle.is_empty() {
            tasks.retain(|t| {
                t.name.to_lowercase().contains(&needle)
                    || t.url.to_lowercase().contains(&needle)
                    || t.status.word().to_lowercase().contains(&needle)
            });
        }
        let col = self.settings.sort_col;
        let desc = self.settings.sort_desc;
        tasks.sort_by(|a, b| {
            let ord = compare(a, b, col);
            if desc {
                ord.reverse()
            } else {
                ord
            }
        });

        let ids: Vec<u64> = tasks.iter().map(|t| t.id).collect();
        let rebuild = ids != self.rows;
        let list = self.ui.list;

        if rebuild {
            let selected = self.selected_ids();
            listview::clear(list);
            for (row, task) in tasks.iter().enumerate() {
                listview::insert_row(list, row as i32, task.id, &cells(task));
            }
            self.rows = ids;
            // keep the user's selection across a rebuild
            for (row, id) in self.rows.iter().enumerate() {
                if selected.contains(id) {
                    listview::focus_row(list, row as i32);
                }
            }
        } else {
            for (row, task) in tasks.iter().enumerate() {
                let values = cells(task);
                for (sub, text) in values.iter().enumerate() {
                    listview::set_cell(list, row as i32, sub as i32, text);
                }
            }
        }

        self.tasks = tasks;
        if was_empty != self.tasks.is_empty() {
            self.layout();
        }
        self.update_chrome();
    }

    pub fn update_chrome(&mut self) {
        let (done, total, running) = self.totals();
        let speed: f64 = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Running)
            .map(|t| t.speed)
            .sum();
        let queued = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Queued)
            .count();
        let finished = self.tasks.iter().filter(|t| t.status == Status::Done).count();

        let paused = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Paused)
            .count();
        let failed = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Failed)
            .count();
        let summary = if self.tasks.is_empty() {
            strings::NO_TASKS.to_string()
        } else {
            let mut s = format!("{} active", running);
            if speed >= 1.0 {
                s.push_str(&format!(" · {}", format::speed(speed)));
            }
            if queued > 0 {
                s.push_str(&format!(" · {} queued", queued));
            }
            if paused > 0 {
                s.push_str(&format!(" · {} paused", paused));
            }
            if failed > 0 {
                s.push_str(&format!(" · {} failed", failed));
            }
            if finished > 0 {
                s.push_str(&format!(" · {} done", finished));
            }
            s
        };
        let summary_for_status = summary.clone();
        self.set_status(0, &summary_for_status);

        let selected = self.selected_ids();
        let detail = match self.tasks.iter().find(|t| selected.contains(&t.id)) {
            None => strings::EMPTY_SELECTION.to_string(),
            Some(task) => detail_line(task),
        };
        self.set_status(1, &detail);
        let free = self.free_space();
        self.set_status(2, &free);

        let (permille, paused, error) = match self.tasks.iter().find(|t| selected.contains(&t.id)) {
            Some(task) => (
                (task.fraction().unwrap_or(0.0) * 1000.0) as u16,
                task.status == Status::Paused,
                task.status == Status::Failed,
            ),
            None => (
                if total > 0 {
                    (done as f64 * 1000.0 / total as f64) as u16
                } else {
                    0
                },
                false,
                false,
            ),
        };
        controls::progress_set(self.ui.progress, permille, paused, error);

        let any_running = self
            .tasks
            .iter()
            .filter(|t| selected.contains(&t.id))
            .any(|t| matches!(t.status, Status::Running | Status::Queued));
        let any_paused = self
            .tasks
            .iter()
            .filter(|t| selected.contains(&t.id))
            .any(|t| matches!(t.status, Status::Paused | Status::Failed));
        unsafe {
            for (i, button) in self.ui.buttons.iter().enumerate() {
                let enabled = match i {
                    1 => any_running,
                    2 => any_paused,
                    3 => !selected.is_empty(),
                    4 => !selected.is_empty(),
                    _ => true,
                };
                let _ = EnableWindow(*button, enabled);
            }
        }
        self.update_taskbar();
    }

    pub fn selected_ids(&self) -> Vec<u64> {
        listview::selected_rows(self.ui.list)
            .into_iter()
            .filter_map(|row| self.rows.get(row as usize).copied())
            .collect()
    }

    /// One row, presented to the user: the ordering must match `strings::COLUMNS`.
    pub fn cells_for(&self) -> Vec<Vec<String>> {
        self.tasks.iter().map(cells).collect()
    }

    pub fn focus_list(&self) {
        unsafe {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(self.ui.list));
        }
    }

    pub fn handle(&self) -> HWND {
        self.hwnd
    }
}

/// One short sentence about the selected row. The table already carries size,
/// speed and ETA, so this stays inside its status-bar part and adds meaning
/// instead of repeating numbers.
fn detail_line(task: &Task) -> String {
    match task.status {
        Status::Done => format!("{} — Completed", task.name),
        Status::Failed => format!(
            "{} — {}",
            task.name,
            task.error.clone().unwrap_or_else(|| "Failed".into())
        ),
        Status::Queued => format!("{} — Queued", task.name),
        Status::Paused => format!(
            "{} — Paused at {} · {} of {}",
            task.name,
            format::percent(task),
            format::size(task.done),
            format::size(task.total)
        ),
        Status::Running => format!(
            "{} — {} · {} · {} left",
            task.name,
            format::percent(task),
            format::speed(task.speed),
            format::eta(task.eta_secs())
        ),
    }
}

fn cells(task: &Task) -> Vec<String> {
    vec![
        task.name.clone(),
        format::size(task.total),
        format::percent(task),
        format::speed(task.speed),
        format::status(task),
        if task.status == Status::Done {
            "—".into()
        } else {
            format::eta(task.eta_secs())
        },
    ]
}

fn compare(a: &Task, b: &Task, col: i32) -> Ordering {
    match col {
        1 => a.total.cmp(&b.total),
        2 => a.fraction().unwrap_or(0.0).total_cmp(&b.fraction().unwrap_or(0.0)),
        3 => a.speed.total_cmp(&b.speed),
        4 => a.status.word().cmp(b.status.word()),
        5 => a.eta_secs().unwrap_or(u64::MAX).cmp(&b.eta_secs().unwrap_or(u64::MAX)),
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    }
}

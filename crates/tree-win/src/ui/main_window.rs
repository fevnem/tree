//! Layout and the periodic refresh: snapshot → rows → chrome.
//!
//! The ListView is only rebuilt when the visible set changes; otherwise cells
//! are updated in place, which keeps an idle Tree at ~0 % CPU.

use super::format;
use crate::app::App;
use crate::win32::{listview, window};
use crate::ui::shell;
use std::cmp::Ordering;
use tree_core::{Status, Task};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

impl App {
    pub fn layout(&mut self) {
        let l = shell::layout(self);
        let pad = self.s(10);

        let filter_w = self.s(240);
        let filter_h = self.s(26);
        window::place(
            self.ui.filter,
            l.toolbar.right - pad - filter_w,
            l.toolbar.top + (l.toolbar.bottom - l.toolbar.top - filter_h) / 2,
            filter_w,
            filter_h,
        );

        let inset = self.s(8);
        let list_x = l.list.left + inset;
        let list_y = l.list.top + inset;
        let list_w = ((l.list.right - l.list.left) - inset * 2).max(self.s(200));
        let list_h = ((l.list.bottom - l.list.top) - inset).max(self.s(80));
        window::place(self.ui.list, list_x, list_y, list_w, list_h);

        // The Name column takes whatever the other five do not need, so the
        // table always fills the pane instead of leaving a phantom column.
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
        let name_width = (list_w - others - 4).max(self.s(240));
        listview::set_column_width(self.ui.list, 0, name_width);
    }

    pub fn refresh(&mut self) {
        let was_empty = self.tasks.is_empty();
        let mut tasks: Vec<Task> = self.engine.snapshot();

        let mut counts = [0usize; 6];
        counts[0] = tasks.len();
        for task in &tasks {
            match task.status {
                Status::Running => counts[1] += 1,
                Status::Queued => counts[2] += 1,
                Status::Done => counts[3] += 1,
                Status::Failed => counts[4] += 1,
                Status::Paused => counts[5] += 1,
            }
        }
        self.counts = counts;
        crate::dbg(&format!("counts: {:?}", counts));
        let filter = self.filter;
        tasks.retain(|t| filter.matches(t.status));
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
            if !tasks.is_empty() && self.selected_ids().is_empty() {
                listview::focus_row(list, 0);
            }
            crate::dbg(&format!(
                "refresh: rows={} selected={:?}",
                self.rows.len(),
                self.selected_ids()
            ));
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

        // Follow the live transfer until the user chooses a row themselves: a
        // task is queued for a moment before it starts, so this cannot be a
        // once-per-rebuild decision.
        if self.follow {
            let selected = self.selected_ids();
            let running = self
                .tasks
                .iter()
                .position(|t| t.status == Status::Running)
                .filter(|i| !selected.contains(&self.tasks[*i].id));
            if let Some(index) = running {
                listview::focus_row(self.ui.list, index as i32);
            }
        }

        let show = !self.tasks.is_empty();
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                self.ui.list,
                if show {
                    windows::Win32::UI::WindowsAndMessaging::SW_SHOW
                } else {
                    windows::Win32::UI::WindowsAndMessaging::SW_HIDE
                },
            );
        }
        if was_empty != self.tasks.is_empty() {
            self.layout();
        }
        self.update_chrome();
    }

    pub fn update_chrome(&mut self) {
        let speed: f64 = self
            .tasks
            .iter()
            .filter(|t| t.status == Status::Running)
            .map(|t| t.speed)
            .sum();
        self.speed_hist.push_back(speed as u64);
        while self.speed_hist.len() > 60 {
            self.speed_hist.pop_front();
        }
        // One repaint of the chrome per tick; the table repaints itself.
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(self.hwnd), None, false);
        }
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

//! Persistent data model shared by the engine and the shell.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Suffix used for the in-progress file. Renamed away on completion.
pub const PART_SUFFIX: &str = ".tree-part";
pub const STATE_VERSION: u32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Running,
    Paused,
    Done,
    Failed,
}

impl Status {
    /// One word for the Status column.
    pub fn word(self) -> &'static str {
        match self {
            Status::Queued => "Queued",
            Status::Running => "Downloading",
            Status::Paused => "Paused",
            Status::Done => "Completed",
            Status::Failed => "Failed",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Status::Done)
    }
}

/// How a task is fetched: our own ranged engine, or yt-dlp for media sites.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Direct,
    YtDlp,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub done: u64,
}

impl Segment {
    pub fn remaining(&self) -> u64 {
        (self.end + 1).saturating_sub(self.start + self.done)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub url: String,
    pub name: String,
    pub dir: String,
    pub total: u64,
    pub done: u64,
    pub supports_ranges: bool,
    pub status: Status,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub segments: Vec<Segment>,
    /// Bytes per second, refreshed by the engine. Not persisted.
    #[serde(skip)]
    pub speed: f64,
}

impl Task {
    pub fn path(&self) -> PathBuf {
        Path::new(&self.dir).join(&self.name)
    }

    pub fn part_path(&self) -> PathBuf {
        Path::new(&self.dir).join(format!("{}{}", self.name, PART_SUFFIX))
    }

    /// 0.0 ..= 1.0, or `None` while the size is unknown.
    pub fn fraction(&self) -> Option<f64> {
        if self.total == 0 {
            None
        } else {
            Some((self.done as f64 / self.total as f64).min(1.0))
        }
    }

    pub fn percent(&self) -> u32 {
        self.fraction().map(|f| (f * 100.0).round() as u32).unwrap_or(0)
    }

    pub fn eta_secs(&self) -> Option<u64> {
        if self.speed < 1.0 || self.total == 0 || self.done >= self.total {
            return None;
        }
        Some(((self.total - self.done) as f64 / self.speed) as u64)
    }

    pub fn done_here(&self) -> bool {
        self.status == Status::Done && self.total > 0 && self.done >= self.total
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Empty means "use the platform default" (set by the shell at startup).
    pub dir: String,
    /// Ranged connections per file (1..=16).
    pub conns: usize,
    /// Files downloading at the same time (1..=5).
    pub simultaneous: usize,
    /// Global cap in bytes/second; 0 = unlimited.
    pub speed_limit: u64,
    pub watch_clipboard: bool,
    pub minimize_to_tray: bool,
    /// One of `theme::THEMES`; "system" follows Windows.
    pub theme: String,
    pub sort_col: i32,
    pub sort_desc: bool,
    pub col_widths: Vec<i32>,
    /// Quiet hours: pause every transfer inside this window and resume after it.
    #[serde(default)]
    pub quiet_enabled: bool,
    /// Minutes since local midnight.
    #[serde(default = "default_quiet_start")]
    pub quiet_start: u16,
    #[serde(default = "default_quiet_end")]
    pub quiet_end: u16,
    /// Window geometry in 96-dpi units; 0 means "let Windows decide".
    pub win_w: i32,
    pub win_h: i32,
    pub win_x: i32,
    pub win_y: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            dir: String::new(),
            conns: 8,
            simultaneous: 3,
            speed_limit: 0,
            watch_clipboard: true,
            // Closing a download manager should close it, not leave an invisible
            // process behind a tray icon nobody asked for.
            minimize_to_tray: false,
            theme: "system".into(),
            sort_col: 0,
            sort_desc: false,
            col_widths: vec![320, 88, 150, 96, 76, 124],
            quiet_enabled: false,
            quiet_start: default_quiet_start(),
            quiet_end: default_quiet_end(),
            win_w: 0,
            win_h: 0,
            win_x: 0,
            win_y: 0,
        }
    }
}

impl Settings {
    pub fn conns(&self) -> usize {
        self.conns.clamp(1, 16)
    }

    pub fn simultaneous(&self) -> usize {
        self.simultaneous.clamp(1, 5)
    }

    pub fn speed_limit(&self) -> u64 {
        self.speed_limit
    }

    /// Keep persisted values sane after a hand-edited state file.
    pub fn sane_columns(&mut self) {
        if self.col_widths.len() != 6 {
            self.col_widths = Settings::default().col_widths;
        }
        for w in &mut self.col_widths {
            *w = (*w).clamp(48, 2000);
        }
        self.conns = self.conns.clamp(1, 16);
        self.simultaneous = self.simultaneous.clamp(1, 5);
        self.win_w = self.win_w.clamp(0, 4000);
        self.win_h = self.win_h.clamp(0, 3000);
    }
}

fn default_quiet_start() -> u16 {
    2 * 60
}

fn default_quiet_end() -> u16 {
    8 * 60
}

impl Settings {
    /// Is `minute` (minutes since midnight) inside the quiet window?
    /// A window that wraps midnight is supported.
    pub fn quiet_at(&self, minute: u16) -> bool {
        if !self.quiet_enabled || self.quiet_start == self.quiet_end {
            return false;
        }
        if self.quiet_start < self.quiet_end {
            minute >= self.quiet_start && minute < self.quiet_end
        } else {
            minute >= self.quiet_start || minute < self.quiet_end
        }
    }
}

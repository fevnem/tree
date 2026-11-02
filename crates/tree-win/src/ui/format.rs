//! Human units. One place, so every column agrees.

use tree_core::{Status, Task};

pub fn size(bytes: u64) -> String {
    if bytes == 0 {
        return "—".into();
    }
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

pub fn speed(bytes_per_second: f64) -> String {
    if bytes_per_second < 1.0 {
        return "—".into();
    }
    format!("{}/s", size(bytes_per_second as u64))
}

pub fn eta(seconds: Option<u64>) -> String {
    match seconds {
        None => "—".into(),
        Some(s) if s >= 3600 => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
        Some(s) if s >= 60 => format!("{}:{:02}", s / 60, s % 60),
        Some(s) => format!("{}s", s),
    }
}

pub fn percent(task: &Task) -> String {
    match task.fraction() {
        Some(f) => format!("{:.0}%", f * 100.0),
        None => "—".into(),
    }
}

pub fn status(task: &Task) -> String {
    match task.status {
        Status::Done => "Completed".into(),
        Status::Failed => "Failed".into(),
        other => other.word().into(),
    }
}

pub fn gib_free(bytes: u64) -> String {
    format!("{:.1} GB free", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

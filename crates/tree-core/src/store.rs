//! state.json: load tolerantly, save atomically.

use crate::model::{Settings, Task, STATE_VERSION};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    settings: Settings,
    tasks: Vec<Task>,
}

/// A missing file is a first run; a corrupt one is moved aside, never fatal.
pub fn load(path: &Path) -> (Settings, Vec<Task>) {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return (Settings::default(), Vec::new()),
    };
    match serde_json::from_str::<Snapshot>(&raw) {
        Ok(mut snap) => {
            snap.settings.sane_columns();
            for (i, t) in snap.tasks.iter_mut().enumerate() {
                if t.id == 0 {
                    t.id = i as u64 + 1;
                }
                t.speed = 0.0;
            }
            (snap.settings, snap.tasks)
        }
        Err(_) => {
            let bad = path.with_extension("json.bad");
            let _ = std::fs::rename(path, bad);
            (Settings::default(), Vec::new())
        }
    }
}

pub fn save(path: &Path, settings: &Settings, tasks: &[Task]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let snap = Snapshot {
        version: STATE_VERSION,
        settings: settings.clone(),
        tasks: tasks.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&snap)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
    }
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(&tmp, path)
}

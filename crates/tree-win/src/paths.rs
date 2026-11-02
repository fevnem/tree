//! Where Tree keeps its files. Portable mode: a `tree.ini` next to the exe moves
//! state there, nothing is written to %APPDATA%.

use std::path::PathBuf;

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn portable() -> bool {
    exe_dir().join("tree.ini").exists()
}

pub fn data_dir() -> PathBuf {
    if portable() {
        return exe_dir();
    }
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("Tree")
}

pub fn state_path() -> PathBuf {
    data_dir().join("state.json")
}

pub fn downloads_dir() -> PathBuf {
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let dl = PathBuf::from(profile).join("Downloads");
        if dl.exists() {
            return dl;
        }
    }
    exe_dir()
}

//! Every string the user can read. One file, so wording stays consistent.

pub const APP_NAME: &str = "Tree";
pub const WINDOW_TITLE: &str = "Tree";

pub const BTN_ADD: &str = "Add";
pub const BTN_PAUSE: &str = "Pause";
pub const BTN_RESUME: &str = "Resume";
pub const BTN_REMOVE: &str = "Remove";
pub const BTN_FOLDER: &str = "Folder";

pub const FILTER_HINT: &str = "Filter";
pub const COLUMNS: [&str; 6] = ["Name", "Size", "Progress", "Speed", "Status", "ETA"];

pub const ADD_TITLE: &str = "Add download";
pub const ADD_URL: &str = "&URL (one per line):";
pub const ADD_DIR: &str = "&Save to:";
pub const ADD_BROWSE: &str = "&Browse…";
pub const ADD_OK: &str = "&Add";
pub const ADD_CANCEL: &str = "Cancel";
pub const PICK_FOLDER: &str = "Choose where Tree saves downloads";

pub const NO_TASKS: &str = "Drop a link here, or press Ctrl+N to add one";
pub const EMPTY_SELECTION: &str = "No download selected";

pub const TRAY_IDLE: &str = "Tree · idle";

pub fn about() -> String {
    format!(
        "{name} {version}\n\nA native Windows download manager.\n\
         Parallel ranged connections, resumable, no runtime to install.\n\
         Video and audio links are handed to yt-dlp when it is available.\n\n\
         Engine: Rust · tokio · SChannel\nUI: Win32\n\nBrowser extension bridge: 127.0.0.1:17817\n",
        name = APP_NAME,
        version = env!("CARGO_PKG_VERSION")
    )
}

pub fn confirm_delete(name: &str) -> String {
    format!("Delete “{}” from disk? This cannot be undone.", name)
}

pub fn tray_tooltip(percent: Option<u32>, active: usize) -> String {
    match (percent, active) {
        (_, 0) => TRAY_IDLE.to_string(),
        (Some(p), 1) => format!("Tree · {} · {}%", "downloading", p),
        (Some(p), n) => format!("Tree · {} active · {}%", n, p),
        (None, n) => format!("Tree · {} active", n),
    }
}

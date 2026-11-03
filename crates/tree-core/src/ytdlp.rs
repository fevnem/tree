//! yt-dlp integration: media links go through the official binary instead of our
//! ranged engine, because sites like YouTube need signature deciphering.
//!
//! Contract with the binary (documented in its README):
//!   * `--progress-template download:TREE|done|total|speed` gives machine-readable
//!     progress on stdout instead of a carriage-return bar (`--newline`).
//!   * `--print after_move:filepath` prints the final path once the file is in
//!     place, which is the only reliable way to learn the real extension after
//!     merging.
//!   * `-c` resumes, so pause/resume maps to kill/restart.

use crate::error::DlError;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const BINARY: &str = "yt-dlp.exe";
pub const RELEASE_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
const PROGRESS_TAG: &str = "TREE|";

/// Live counters for one yt-dlp task; the engine mirrors them into the task list.
#[derive(Default)]
pub struct Ctl {
    pub stop: AtomicU8,
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub speed: AtomicU64,
}

impl Ctl {
    pub fn reset(&self) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.speed.store(0, Ordering::Relaxed);
        // `stop` is intentionally left alone: the caller owns it.
    }
}

/// Where the binary can live: next to tree.exe (portable), in our data folder
/// (installed by Tools ▸ Download yt-dlp), or anywhere on PATH.
pub fn find(exe_dir: &Path, data_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        exe_dir.join(BINARY),
        data_dir.join("bin").join(BINARY),
        data_dir.join(BINARY),
    ];
    for path in candidates {
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(BINARY);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Hosts that never serve a plain file, so they must go through yt-dlp.
const MEDIA_HOSTS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "music.youtube.com",
    "vimeo.com",
    "dailymotion.com",
    "twitch.tv",
    "soundcloud.com",
    "facebook.com",
    "instagram.com",
    "tiktok.com",
    "twitter.com",
    "x.com",
    "reddit.com",
    "bilibili.com",
    "nicovideo.jp",
    "rumble.com",
    "odysee.com",
    "streamable.com",
    "vk.com",
    "bitchute.com",
];

pub fn is_media_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("").trim_start_matches("www.");
    MEDIA_HOSTS
        .iter()
        .any(|m| host == *m || host.ends_with(&format!(".{}", m)))
}

/// Ask the binary what the file will be called, without downloading anything.
pub fn query_name(exe: &Path, url: &str) -> Option<String> {
    let output = Command::new(exe)
        .args([
            "--no-warnings",
            "--no-playlist",
            "--no-colors",
            "--windows-filenames",
            "--print",
            "filename",
            "-o",
            "%(title)s.%(ext)s",
        ])
        .arg(url)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let name = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(crate::plan::sanitize(name))
}

fn parse_progress(line: &str) -> Option<(u64, u64, u64)> {
    let rest = line.trim().strip_prefix(PROGRESS_TAG)?;
    let mut parts = rest.split('|');
    let done = parts.next()?.trim().parse::<u64>().unwrap_or(0);
    let total = parts.next()?.trim().parse::<u64>().unwrap_or(0);
    let speed = parts
        .next()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    Some((done, total, speed.max(0.0) as u64))
}

/// Run one download to completion. Blocking: called from a worker thread.
pub fn download(
    exe: &Path,
    url: &str,
    dir: &Path,
    ctl: &Arc<Ctl>,
) -> Result<PathBuf, DlError> {
    ctl.reset();
    // ffmpeg is expected beside yt-dlp.exe; naming the folder keeps yt-dlp from
    // hunting PATH and silently skipping the merge.
    let bin_dir = exe
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut child = Command::new(exe)
        .args([
            "--newline",
            "--no-colors",
            "--no-playlist",
            "--windows-filenames",
            "--continue",
            // Best video + best audio, merged, with a predictable container.
            "-f",
            "bv*+ba/b",
            "--merge-output-format",
            "mp4",
            "--progress-delta",
            "0.5",
            "--progress-template",
            "download:TREE|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.speed)s",
            "--print",
            "after_move:filepath",
            "-P",
        ])
        .arg(dir)
        // ffmpeg lives next to yt-dlp; without it yt-dlp warns, exits 0 and
        // leaves unmerged fragments, so the location is explicit.
        .args(["--ffmpeg-location", &bin_dir])
        .args(["-o", "%(title)s.%(ext)s"])
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| DlError::Disk(format!("yt-dlp: {}", e)))?;

    // stderr is drained on its own thread, otherwise a chatty run fills the pipe
    // and the process deadlocks.
    let tail = Arc::new(Mutex::new(String::new()));
    if let Some(mut err) = child.stderr.take() {
        let tail = tail.clone();
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = err.read_to_string(&mut buf);
            let mut slot = tail.lock().unwrap_or_else(|e| e.into_inner());
            *slot = buf;
        });
    }

    let mut final_path: Option<PathBuf> = None;
    let mut last_tick = Instant::now();
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if ctl.stop.load(Ordering::Relaxed) != 0 {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DlError::Cancelled);
            }
            if let Some((done, total, speed)) = parse_progress(&line) {
                ctl.done.store(done, Ordering::Relaxed);
                if total > 0 {
                    ctl.total.store(total, Ordering::Relaxed);
                }
                ctl.speed.store(speed, Ordering::Relaxed);
                last_tick = Instant::now();
                continue;
            }
            // after_move prints the absolute path once the file exists.
            let trimmed = line.trim();
            if trimmed.len() > 3 && (trimmed.contains(":\\") || trimmed.contains(":/")) {
                final_path = Some(PathBuf::from(trimmed));
            }
        }
    }
    let _ = last_tick;

    let status = child
        .wait()
        .map_err(|e| DlError::Disk(format!("yt-dlp: {}", e)))?;
    let stderr = {
        let slot = tail.lock().unwrap_or_else(|e| e.into_inner());
        slot.clone()
    };

    if ctl.stop.load(Ordering::Relaxed) != 0 {
        return Err(DlError::Cancelled);
    }
    if !status.success() {
        // yt-dlp exits 1 when its final rename loses a race with another process
        // holding the same file name, even though every byte arrived. Treat that
        // as success when the transfer was complete and the file is on disk.
        let done = ctl.done.load(Ordering::Relaxed);
        let total = ctl.total.load(Ordering::Relaxed);
        let recovered = complete_despite_error(&stderr, done, total)
            .then(|| final_path.clone().filter(|p| p.exists()))
            .flatten();
        if let Some(path) = recovered {
            return Ok(path);
        }
        let message = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty() && !l.trim().starts_with("WARNING"))
            .unwrap_or("yt-dlp failed");
        return Err(DlError::Network(message.trim().to_string()));
    }

    let path = final_path.ok_or_else(|| {
        DlError::Disk("yt-dlp finished but did not report a file path".into())
    })?;
    // Exit 0 and a printed path prove nothing: with ffmpeg missing, yt-dlp
    // prints the merged name while only the fragments exist.
    if !path.exists() {
        return Err(DlError::Disk(
            "yt-dlp reported a file it did not produce (ffmpeg missing?)".into(),
        ));
    }
    // Freeze the counters at 100 %.
    if let Ok(meta) = std::fs::metadata(&path) {
        ctl.total.store(meta.len(), Ordering::Relaxed);
        ctl.done.store(meta.len(), Ordering::Relaxed);
    }
    Ok(path)
}

/// Download the official Windows binary into `target` (used by Tools ▸ yt-dlp).
pub fn install(target: &Path) -> Result<u64, DlError> {
    use futures_util::StreamExt;
    use std::io::Write;

    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| DlError::Disk(e.to_string()))?;

    rt.block_on(async {
        let client = crate::http_client();
        let resp = client.get(RELEASE_URL).send().await?;
        if !resp.status().is_success() {
            return Err(DlError::Http(resp.status().as_u16()));
        }
        let tmp = target.with_extension("exe.download");
        let mut file = std::fs::File::create(&tmp)?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?)?;
        }
        file.sync_all()?;
        drop(file);
        if target.exists() {
            let _ = std::fs::remove_file(target);
        }
        std::fs::rename(&tmp, target)?;
        Ok(std::fs::metadata(target).map(|m| m.len()).unwrap_or(0))
    })
}

/// Startup sweep: mark a stalled progress reader as stopped.
pub fn sleep_tick() {
    std::thread::sleep(Duration::from_millis(250));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_collision_after_completion_is_success() {
        let stderr = "ERROR: Unable to rename file: [Errno 2] big.mp4.part -> big.mp4";
        assert!(complete_despite_error(stderr, 100, 100));
    }

    #[test]
    fn rename_collision_before_completion_is_failure() {
        let stderr = "ERROR: Unable to rename file: [Errno 2] big.mp4.part -> big.mp4";
        assert!(!complete_despite_error(stderr, 40, 100));
    }

    #[test]
    fn unrelated_errors_stay_errors() {
        assert!(!complete_despite_error("ERROR: Video unavailable", 100, 100));
        assert!(!complete_despite_error("", 100, 100));
    }

    #[test]
    fn recognises_media_hosts() {
        assert!(is_media_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_media_url("https://youtu.be/abc"));
        assert!(is_media_url("https://music.youtube.com/watch?v=abc"));
        assert!(!is_media_url("https://example.com/file.zip"));
        assert!(!is_media_url("not a url"));
    }

    #[test]
    fn parses_progress_lines() {
        assert_eq!(
            parse_progress("TREE|1024|2048|512.5"),
            Some((1024, 2048, 512))
        );
        assert_eq!(parse_progress("TREE|NA|NA|NA"), Some((0, 0, 0)));
        assert_eq!(parse_progress("[download] 12% of 3MiB"), None);
    }
}

/// yt-dlp reports failure when its final rename collides with another process,
/// even after the whole file arrived. The bytes are what matter, so treat a
/// complete transfer whose only complaint is the rename as done.
fn complete_despite_error(stderr: &str, done: u64, total: u64) -> bool {
    let rename_conflict = stderr.contains("Unable to rename file");
    rename_conflict && total > 0 && done >= total
}

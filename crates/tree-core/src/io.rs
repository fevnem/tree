//! File plumbing: one preallocated part file, written at absolute offsets by
//! every connection at once (no shared cursor, no locking in the write path).

use crate::model::PART_SUFFIX;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::FileExt;
#[cfg(unix)]
use std::os::unix::fs::FileExt;

const GROW_STEP: u64 = 8 << 20;

/// Create (or reopen) the part file and try to preallocate `total` bytes.
///
/// Preallocation is best effort: on exFAT, network shares or a quota it fails
/// and we fall back to growing the file as data arrives.
pub fn create_part(path: &Path, total: u64) -> io::Result<File> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if total > 0 {
        let have = file.metadata().map(|m| m.len()).unwrap_or(0);
        if have < total {
            let _ = file.set_len(total);
        }
    }
    Ok(file)
}

pub fn open_part(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open(path)
}

/// Write `buf` at `offset` from multiple connections concurrently.
pub fn write_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    let mut written = 0usize;
    while written < buf.len() {
        #[cfg(windows)]
        let n = file.seek_write(&buf[written..], offset + written as u64)?;
        #[cfg(unix)]
        let n = file.write_at(&buf[written..], offset + written as u64)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "short write while saving",
            ));
        }
        written += n;
    }
    Ok(())
}

/// Ensure the file is at least `need` bytes long (used when preallocation failed).
pub fn ensure_len(file: &File, need: u64) -> io::Result<()> {
    let have = file.metadata()?.len();
    if have < need {
        file.set_len(need.min(have + GROW_STEP.max(need - have)))?;
    }
    Ok(())
}

/// Trim to the exact size, flush, then rename into place.
pub fn finalize(part: &Path, dest: &Path, total: u64) -> io::Result<()> {
    if let Some(dir) = dest.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    {
        let file = OpenOptions::new().write(true).open(part)?;
        if total > 0 {
            let have = file.metadata()?.len();
            if have != total {
                file.set_len(total)?;
            }
        }
        file.sync_all()?;
    }
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(part, dest)
}

/// Delete the part file if present (used by Remove).
pub fn discard_part(dir: &Path, name: &str) {
    let p: PathBuf = dir.join(format!("{}{}", name, PART_SUFFIX));
    let _ = std::fs::remove_file(p);
}

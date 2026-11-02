//! Naming and segmentation rules. Pure functions, unit tested.

use crate::model::{Segment, PART_SUFFIX};
use percent_encoding::percent_decode_str;
use std::path::Path;

/// Connections actually used: never more than one per 2 MiB, never more than
/// `conns`, and only when the server honoured a ranged request.
pub fn segment_count(total: u64, conns: usize, ranges: bool) -> usize {
    if !ranges || total == 0 || total < (1 << 20) {
        return 1;
    }
    let n = (total / (2 << 20)) as usize;
    n.clamp(1, conns.clamp(1, 16))
}

pub fn split_segments(total: u64, n: usize) -> Vec<Segment> {
    if total == 0 {
        return vec![Segment {
            start: 0,
            end: u64::MAX,
            done: 0,
        }];
    }
    let n = n.max(1) as u64;
    let base = total / n;
    let mut out = Vec::with_capacity(n as usize);
    let mut start = 0u64;
    for i in 0..n {
        let end = if i == n - 1 { total - 1 } else { start + base - 1 };
        out.push(Segment {
            start,
            end,
            done: 0,
        });
        start = end + 1;
    }
    out
}

/// DOS reserved device names — a file called `CON.txt` cannot be created.
fn is_reserved(stem: &str) -> bool {
    let up = stem.to_ascii_uppercase();
    matches!(up.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((up.starts_with("COM") || up.starts_with("LPT"))
            && up.len() == 4
            && up.as_bytes()[3].is_ascii_digit())
}

/// Make any server-supplied name safe on NTFS.
pub fn sanitize(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();
    let mut trimmed = cleaned.trim().trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() {
        return "download".into();
    }
    // keep the extension while shortening
    if trimmed.chars().count() > 180 {
        let (stem, ext) = match trimmed.rfind('.') {
            Some(i) if trimmed.len() - i <= 16 => trimmed.split_at(i),
            _ => (trimmed.as_str(), ""),
        };
        let keep: String = stem.chars().take(160).collect();
        trimmed = format!("{}{}", keep, ext);
    }
    let stem = trimmed.split('.').next().unwrap_or(&trimmed);
    if is_reserved(stem) {
        trimmed = format!("_{}", trimmed);
    }
    trimmed
}

pub fn name_from_url(url: &str) -> String {
    let parsed = url::Url::parse(url).ok();
    if let Some(u) = parsed {
        let last = u
            .path_segments()
            .and_then(|mut s| s.rfind(|p| !p.is_empty()))
            .unwrap_or("");
        if !last.is_empty() {
            let decoded = percent_decode_str(last).decode_utf8_lossy().to_string();
            return sanitize(&decoded);
        }
        if let Some(host) = u.host_str() {
            return sanitize(&format!("{}-download", host));
        }
    }
    "download".into()
}

/// RFC 6266 / 5987: `attachment; filename*=UTF-8''name.pdf; filename="name.pdf"`
pub fn name_from_disposition(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    // extended form wins
    if let Some(idx) = lower.find("filename*=") {
        let rest = &value[idx + "filename*=".len()..];
        let token = rest.split(';').next().unwrap_or(rest).trim();
        let after = match token.find("''") {
            Some(i) => &token[i + 2..],
            None => token,
        };
        let decoded = percent_decode_str(after).decode_utf8_lossy().to_string();
        if !decoded.trim().is_empty() {
            return Some(sanitize(&decoded));
        }
    }
    if let Some(idx) = lower.find("filename=") {
        let rest = &value[idx + "filename=".len()..];
        let token = rest.split(';').next().unwrap_or(rest).trim();
        let unquoted = token.trim_matches('"');
        if !unquoted.trim().is_empty() {
            return Some(sanitize(unquoted));
        }
    }
    None
}

pub fn ext_for_mime(mime: &str) -> Option<&'static str> {
    let m = mime.split(';').next().unwrap_or(mime).trim();
    Some(match m {
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        "application/x-7z-compressed" => "7z",
        "application/x-rar-compressed" => "rar",
        "application/gzip" | "application/x-gzip" => "gz",
        "application/x-tar" => "tar",
        "application/x-iso9660-image" => "iso",
        "application/vnd.android.package-archive" => "apk",
        "application/x-msdownload" | "application/vnd.microsoft.portable-executable" => "exe",
        "application/msword" => "doc",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.ms-excel" => "xls",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "audio/mpeg" => "mp3",
        "audio/flac" => "flac",
        "video/mp4" => "mp4",
        "video/x-matroska" => "mkv",
        "text/plain" => "txt",
        "text/html" => "html",
        _ => return None,
    })
}

/// `name (2).ext` until free — checking both the final file and its part file.
pub fn unique_name(dir: &Path, desired: &str) -> String {
    let desired = sanitize(desired);
    if !taken(dir, &desired) {
        return desired;
    }
    let (stem, ext) = match desired.rfind('.') {
        Some(i) if i > 0 && desired.len() - i <= 16 => (&desired[..i], &desired[i..]),
        _ => (desired.as_str(), ""),
    };
    // "movie (2).mkv" that is taken becomes "movie (3).mkv", never "movie (2) (2).mkv".
    let (base, first) = match stem.rfind(" (") {
        Some(i) if stem.ends_with(')') => match stem[i + 2..stem.len() - 1].parse::<u32>() {
            Ok(n) if n >= 2 => (&stem[..i], n + 1),
            _ => (stem, 2),
        },
        _ => (stem, 2),
    };
    for n in first..10_000u32 {
        let candidate = format!("{} ({}){}", base, n, ext);
        if !taken(dir, &candidate) {
            return candidate;
        }
    }
    format!("{}-{}{}", base, std::process::id(), ext)
}

fn taken(dir: &Path, name: &str) -> bool {
    dir.join(name).exists() || dir.join(format!("{}{}", name, PART_SUFFIX)).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_cover_every_byte_once() {
        let segs = split_segments(1000, 4);
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs[3].end, 999);
        for pair in segs.windows(2) {
            assert_eq!(pair[0].end + 1, pair[1].start);
        }
        let sum: u64 = segs.iter().map(|s| s.end - s.start + 1).sum();
        assert_eq!(sum, 1000);
    }

    #[test]
    fn segment_count_is_polite() {
        assert_eq!(segment_count(0, 8, true), 1);
        assert_eq!(segment_count(4096, 8, true), 1);
        assert_eq!(segment_count(64 << 20, 8, false), 1);
        assert_eq!(segment_count(64 << 20, 8, true), 8);
        assert_eq!(segment_count(64 << 20, 32, true), 16);
    }

    #[test]
    fn sanitizes_hostile_names() {
        assert_eq!(sanitize("a/b\\c:d*e?f"), "a_b_c_d_e_f");
        assert_eq!(sanitize("CON.txt"), "_CON.txt");
        assert_eq!(sanitize("  "), "download");
        assert!(!sanitize(&"x".repeat(400)).is_empty());
    }

    #[test]
    fn reads_names_from_urls_and_headers() {
        assert_eq!(
            name_from_url("https://example.com/files/report%20final.pdf?x=1"),
            "report final.pdf"
        );
        assert_eq!(name_from_url("https://example.com/"), "example.com-download");
        assert_eq!(
            name_from_disposition("attachment; filename=\"setup.exe\"").as_deref(),
            Some("setup.exe")
        );
        assert_eq!(
            name_from_disposition("attachment; filename*=UTF-8''na%C3%AFve%20file.zip").as_deref(),
            Some("naïve file.zip")
        );
    }

    #[test]
    fn collisions_do_not_clobber() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("movie.mkv"), b"x").unwrap();
        assert_eq!(unique_name(dir.path(), "movie.mkv"), "movie (2).mkv");
        // an in-flight part file blocks the name too
        std::fs::write(dir.path().join("movie (2).mkv.tree-part"), b"x").unwrap();
        assert_eq!(unique_name(dir.path(), "movie (2).mkv"), "movie (3).mkv");
        std::fs::write(dir.path().join("movie.tree-part"), b"x").unwrap();
        assert_eq!(unique_name(dir.path(), "movie.mkv"), "movie (3).mkv");
    }
}

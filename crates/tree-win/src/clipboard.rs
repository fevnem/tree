//! Clipboard: read a link, or put one back.

use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Current clipboard text, if any.
pub fn read_text() -> Option<String> {
    unsafe {
        OpenClipboard(None).ok()?;
        let mut out = None;
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_ok() {
            if let Ok(handle) = GetClipboardData(CF_UNICODETEXT.0 as u32) {
                let ptr = GlobalLock(HGLOBAL(handle.0));
                if !ptr.is_null() {
                    let wide = ptr as *const u16;
                    let mut len = 0usize;
                    while len < 32_768 && *wide.add(len) != 0 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(wide, len);
                    out = Some(String::from_utf16_lossy(slice));
                    let _ = GlobalUnlock(HGLOBAL(handle.0));
                }
            }
        }
        let _ = CloseClipboard();
        out
    }
}

/// First http(s) URL in `text`, trimmed to one line.
pub fn first_url(text: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("http://") || t.starts_with("https://") {
            return Some(t.split_whitespace().next().unwrap_or(t).to_string());
        }
    }
    None
}

pub fn write_text(text: &str) {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        let bytes = wide.len() * 2;
        if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
            let ptr = GlobalLock(mem);
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(
                    wide.as_ptr() as *const u8,
                    ptr as *mut u8,
                    bytes,
                );
                let _ = GlobalUnlock(mem);
                let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(mem.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

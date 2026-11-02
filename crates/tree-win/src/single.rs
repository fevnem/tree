//! Single instance, done the way Win32 apps do it: look for the window, prove it
//! is alive, hand over any links, exit. No mutex — a mutex left behind by a
//! killed process makes the app refuse to start, which is worse than the
//! one-second race it would prevent.

use crate::win32::{wide, window};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SendMessageTimeoutW, SetForegroundWindow, ShowWindow, SMTO_ABORTIFHUNG,
    SMTO_BLOCK, SW_RESTORE, WM_COPYDATA, WM_NULL,
};

const COPYDATA_URLS: usize = 0x5452_4545; // 'TREE'
const PROBE_MS: u32 = 300;
const ATTEMPTS: u32 = 20;

pub fn find_running() -> Option<HWND> {
    unsafe { FindWindowW(window::MAIN_CLASS, None).ok() }
}

/// A window that does not answer a no-op message belongs to a dead process.
fn responds(hwnd: HWND) -> bool {
    let mut out: usize = 0;
    unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            PROBE_MS,
            Some(&mut out),
        )
        .0 != 0
    }
}

/// True when an existing Tree took the URLs (or was brought forward), so this
/// launch should exit.
pub fn hand_over(urls: &[String]) -> bool {
    for _ in 0..ATTEMPTS {
        if let Some(hwnd) = find_running() {
            if responds(hwnd) {
                deliver(hwnd, urls);
                unsafe {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

fn deliver(hwnd: HWND, urls: &[String]) {
    if urls.is_empty() {
        return;
    }
    let mut payload: Vec<u16> = Vec::new();
    for url in urls {
        payload.extend(url.encode_utf16());
        payload.push(b'\n' as u16);
    }
    payload.push(0);
    let data = COPYDATASTRUCT {
        dwData: COPYDATA_URLS,
        cbData: (payload.len() * 2) as u32,
        lpData: payload.as_ptr() as *mut core::ffi::c_void,
    };
    let mut out: usize = 0;
    unsafe {
        // Never hang on a wedged instance.
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&data as *const COPYDATASTRUCT as isize),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            1000,
            Some(&mut out),
        );
    }
}

/// Parse a WM_COPYDATA payload back into URLs.
pub unsafe fn read_copy_data(lparam: LPARAM) -> Vec<String> {
    let data = unsafe { &*(lparam.0 as *const COPYDATASTRUCT) };
    if data.dwData != COPYDATA_URLS || data.lpData.is_null() {
        return Vec::new();
    }
    let len = data.cbData as usize / 2;
    let slice = unsafe { std::slice::from_raw_parts(data.lpData as *const u16, len) };
    let text = wide::from_wide(slice);
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

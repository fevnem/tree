//! One window. A second launch hands its URL to the running instance and exits.

use crate::win32::{wide, window};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SendMessageW, SetForegroundWindow, ShowWindow, SW_RESTORE, WM_COPYDATA,
};

const COPYDATA_URLS: usize = 0x5452_4545; // 'TREE'

pub fn find_running() -> Option<HWND> {
    unsafe { FindWindowW(window::MAIN_CLASS, None).ok() }
}

/// Returns true when the URLs were handed over and this process should exit.
pub fn forward(urls: &[String]) -> bool {
    if urls.is_empty() {
        return false;
    }
    let Some(hwnd) = find_running() else {
        return false;
    };
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
    unsafe {
        SendMessageW(
            hwnd,
            WM_COPYDATA,
            Some(WPARAM(0)),
            Some(LPARAM(&data as *const COPYDATASTRUCT as isize)),
        );
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
    true
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

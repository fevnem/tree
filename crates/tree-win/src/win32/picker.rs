//! The system folder picker (SHBrowseForFolder with the modern style flag).

use super::wide;
use windows::core::PWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
};

pub fn pick_folder(owner: HWND, title: &str) -> Option<String> {
    unsafe {
        let mut display = [0u16; 260];
        let title_w = wide::wide(title);
        let info = BROWSEINFOW {
            hwndOwner: owner,
            pidlRoot: std::ptr::null_mut(),
            pszDisplayName: PWSTR(display.as_mut_ptr()),
            lpszTitle: wide::pcw(&title_w),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: None,
            lParam: windows::Win32::Foundation::LPARAM(0),
            iImage: 0,
        };
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return None;
        }
        let mut path = [0u16; 260];
        let ok = SHGetPathFromIDListW(pidl, &mut path).as_bool();
        CoTaskMemFree(Some(pidl as *const core::ffi::c_void));
        if ok {
            let s = wide::from_wide(&path);
            if !s.is_empty() {
                return Some(s);
            }
        }
        None
    }
}

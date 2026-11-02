//! Window class registration, creation and small geometry helpers.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows::Win32::Graphics::Gdi::{HBRUSH, COLOR_WINDOW};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetClientRect, MoveWindow, RegisterClassExW, HICON, HMENU, WINDOW_EX_STYLE,
    WINDOW_STYLE, WNDCLASSEXW, WNDPROC,
};

pub fn instance() -> HINSTANCE {
    unsafe {
        let module = GetModuleHandleW(None).unwrap_or_default();
        HINSTANCE(module.0)
    }
}

pub fn system_brush() -> HBRUSH {
    // (COLOR_WINDOW + 1) is the documented way to ask for the system colour brush.
    HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut core::ffi::c_void)
}

pub fn register_class(name: PCWSTR, icon: Option<HICON>, proc_: WNDPROC, brush: HBRUSH) -> bool {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: proc_,
            hInstance: instance(),
            hIcon: icon.unwrap_or_default(),
            hIconSm: icon.unwrap_or_default(),
            hbrBackground: brush,
            lpszClassName: name,
            ..Default::default()
        };
        RegisterClassExW(&wc) != 0
    }
}

pub fn create(
    class: PCWSTR,
    title: &str,
    style: WINDOW_STYLE,
    ex: WINDOW_EX_STYLE,
    menu: Option<HMENU>,
    w: i32,
    h: i32,
) -> windows::core::Result<HWND> {
    let t = super::wide::wide(title);
    unsafe {
        CreateWindowExW(
            ex,
            class,
            super::wide::pcw(&t),
            style,
            100,
            80,
            w,
            h,
            None,
            menu,
            Some(instance()),
            None,
        )
    }
}

pub fn client_rect(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
    }
    r
}

pub fn place(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        let _ = MoveWindow(hwnd, x, y, w, h, true);
    }
}

/// Class names used across the shell.
pub const MAIN_CLASS: PCWSTR = w!("TreeMainWindow");
pub const ADD_CLASS: PCWSTR = w!("TreeAddDialog");

//! Windows look: the shell's own message font, immersive dark title bar,
//! dark Explorer theming for the list, and the two colours we paint with.

use super::window;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, COLORREF};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{CreateFontIndirectW, CreateSolidBrush, HFONT, HGDIOBJ, HBRUSH};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
};
use windows::Win32::UI::Controls::SetWindowTheme;
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

/// Segoe UI at the size the OS itself uses — the single biggest "this is a real
/// Windows app" detail.
#[allow(clippy::field_reassign_with_default)]
pub fn message_font() -> HFONT {
    unsafe {
        let mut metrics = NONCLIENTMETRICSW::default();
        metrics.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
        let ok = SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            Some(&mut metrics as *mut _ as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        if ok.is_ok() {
            let font = CreateFontIndirectW(&metrics.lfMessageFont);
            if !font.0.is_null() {
                return font;
            }
        }
        HFONT::default()
    }
}

/// Reads HKCU\...\Themes\Personalize\AppsUseLightTheme.
pub fn system_is_dark() -> bool {
    unsafe {
        let mut value: u32 = 1;
        let mut size: u32 = 4;
        let rc = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            Some(&mut size),
        );
        rc.0 == 0 && value == 0
    }
}

pub fn frame(hwnd: HWND, dark: bool) {
    let value: i32 = if dark { 1 } else { 0 };
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &value as *const i32 as *const core::ffi::c_void,
            4,
        );
    }
}

pub fn explorer_theme(hwnd: HWND, dark: bool) {
    unsafe {
        let name = if dark {
            w!("DarkMode_Explorer")
        } else {
            PCWSTR::null()
        };
        let _ = SetWindowTheme(hwnd, name, PCWSTR::null());
    }
}

pub struct Palette {
    pub background: HBRUSH,
    pub text: COLORREF,
    pub owns_brush: bool,
}

impl Palette {
    pub fn for_dark(dark: bool) -> Palette {
        if dark {
            unsafe {
                Palette {
                    background: CreateSolidBrush(COLORREF(0x0019_1919)),
                    text: COLORREF(0x00e6_e6e6),
                    owns_brush: true,
                }
            }
        } else {
            Palette {
                background: window::system_brush(),
                text: COLORREF(0x0000_0000),
                owns_brush: false,
            }
        }
    }

    pub fn dispose(&self) {
        if self.owns_brush && !self.background.0.is_null() {
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(HGDIOBJ(self.background.0));
            }
        }
    }
}

//! Themes. Windows only lets an app recolour what it paints itself, so each
//! theme sets the colours we own (window, list, text) and tells the OS whether
//! the chrome (title bar, menus, buttons, status bar) should be light or dark.
//!
//! That split is deliberate: inventing colours for stock controls would mean
//! owner-drawing every one of them, which is what makes "native" apps look fake.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{CreateFontIndirectW, CreateSolidBrush, HBRUSH, HFONT, HGDIOBJ};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Controls::{
    SetWindowTheme, LVM_GETHEADER, LVM_SETBKCOLOR, LVM_SETTEXTBKCOLOR, LVM_SETTEXTCOLOR,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageW, SystemParametersInfoW, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

/// (key, menu label)
pub const THEMES: [(&str, &str); 10] = [
    ("system", "Follow Windows"),
    ("light", "Light"),
    ("dark", "Dark"),
    ("graphite", "Graphite"),
    ("midnight", "Midnight"),
    ("nord", "Nord"),
    ("solarized", "Solarized"),
    ("sepia", "Sepia"),
    ("rose", "Rosé"),
    ("forest", "Forest"),
];

#[derive(Clone, Copy)]
pub struct Colors {
    pub bg: u32,
    pub surface: u32,
    pub text: u32,
    pub dim: u32,
    /// Used for the progress bar fill, so each theme has one accent.
    pub accent: u32,
    pub dark: bool,
}

pub fn palette_for(theme: &str) -> Colors {
    match theme {
        "light" => Colors {
            bg: rgb(247, 247, 247),
            surface: rgb(255, 255, 255),
            text: rgb(26, 26, 26),
            dim: rgb(107, 107, 107),
            accent: rgb(0, 120, 215),
            dark: false,
        },
        "dark" => Colors {
            bg: rgb(28, 28, 28),
            surface: rgb(32, 32, 32),
            text: rgb(230, 230, 230),
            dim: rgb(172, 174, 180),
            accent: rgb(76, 141, 255),
            dark: true,
        },
        "graphite" => Colors {
            bg: rgb(36, 38, 41),
            surface: rgb(45, 48, 52),
            text: rgb(214, 217, 222),
            dim: rgb(138, 143, 150),
            accent: rgb(120, 170, 255),
            dark: true,
        },
        "midnight" => Colors {
            bg: rgb(16, 24, 38),
            surface: rgb(22, 32, 52),
            text: rgb(215, 225, 245),
            dim: rgb(122, 140, 173),
            accent: rgb(96, 148, 255),
            dark: true,
        },
        "nord" => Colors {
            bg: rgb(46, 52, 64),
            surface: rgb(59, 66, 82),
            text: rgb(236, 239, 244),
            dim: rgb(160, 170, 185),
            accent: rgb(136, 192, 208),
            dark: true,
        },
        "solarized" => Colors {
            bg: rgb(0, 43, 54),
            surface: rgb(7, 54, 66),
            text: rgb(238, 232, 213),
            dim: rgb(131, 148, 150),
            accent: rgb(38, 139, 210),
            dark: true,
        },
        "sepia" => Colors {
            bg: rgb(244, 236, 216),
            surface: rgb(239, 230, 208),
            text: rgb(59, 50, 38),
            dim: rgb(122, 108, 88),
            accent: rgb(150, 100, 30),
            dark: false,
        },
        "rose" => Colors {
            bg: rgb(42, 30, 36),
            surface: rgb(53, 37, 45),
            text: rgb(242, 221, 228),
            dim: rgb(168, 138, 150),
            accent: rgb(214, 122, 158),
            dark: true,
        },
        "forest" => Colors {
            bg: rgb(24, 35, 28),
            surface: rgb(32, 46, 37),
            text: rgb(222, 235, 225),
            dim: rgb(130, 155, 138),
            accent: rgb(110, 180, 130),
            dark: true,
        },
        // "system": follow the OS choice, with our own tints for each.
        _ => {
            if system_is_dark() {
                palette_for("dark")
            } else {
                Colors {
                    bg: rgb(247, 247, 247),
                    surface: rgb(255, 255, 255),
                    text: rgb(26, 26, 26),
                    dim: rgb(107, 107, 107),
                    accent: rgb(0, 120, 215),
                    dark: false,
                }
            }
        }
    }
}

/// Segoe UI at the size the OS itself uses.
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

/// Dark title bar / frame.
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

/// Chrome follow light/dark; the theme name is a fixed string, never a pointer
/// into a temporary buffer.
pub fn control(hwnd: HWND, dark: bool) {
    unsafe {
        let theme = if dark {
            w!("DarkMode_Explorer")
        } else {
            PCWSTR::null()
        };
        let _ = SetWindowTheme(hwnd, theme, PCWSTR::null());
    }
}


/// A ListView keeps its own colours; theming alone does not paint its body on
/// every Windows build, so set them explicitly too.
pub fn listview(lv: HWND, colors: Colors) {
    control(lv, colors.dark);
    let bg = COLORREF(colors.surface);
    let fg = COLORREF(colors.text);
    unsafe {
        SendMessageW(lv, LVM_SETBKCOLOR, Some(WPARAM(0)), Some(LPARAM(bg.0 as isize)));
        SendMessageW(
            lv,
            LVM_SETTEXTBKCOLOR,
            Some(WPARAM(0)),
            Some(LPARAM(bg.0 as isize)),
        );
        SendMessageW(lv, LVM_SETTEXTCOLOR, Some(WPARAM(0)), Some(LPARAM(fg.0 as isize)));

        let header = SendMessageW(lv, LVM_GETHEADER, Some(WPARAM(0)), Some(LPARAM(0)));
        let header = HWND(header.0 as *mut core::ffi::c_void);
        if !header.is_invalid() {
            control(header, colors.dark);
        }
    }
}

pub struct Palette {
    pub colors: Colors,
    pub background: HBRUSH,
    pub surface: HBRUSH,
    pub text: COLORREF,
    owned: bool,
}

impl Palette {
    pub fn new(colors: Colors) -> Palette {
        unsafe {
            Palette {
                colors,
                background: CreateSolidBrush(COLORREF(colors.bg)),
                surface: CreateSolidBrush(COLORREF(colors.surface)),
                text: COLORREF(colors.text),
                owned: true,
            }
        }
    }

    pub fn dispose(&self) {
        if !self.owned {
            return;
        }
        for brush in [self.background, self.surface] {
            if !brush.0.is_null() {
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::DeleteObject(HGDIOBJ(brush.0));
                }
            }
        }
    }
}


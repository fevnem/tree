//! Report-view ListView: the table. Real SysListView32 — OS theming, sorting,
//! keyboard, accessibility and high-contrast all come for free.

use super::wide;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Controls::{
    LVCF_FMT, LVCF_SUBITEM, LVCF_TEXT, LVCF_WIDTH, LVCOLUMNW, LVCFMT_LEFT, LVIF_PARAM, LVIF_TEXT,
    LVITEMW, LVM_DELETEALLITEMS, LVM_GETNEXTITEM, LVM_INSERTCOLUMNW, LVM_INSERTITEMW,
    LVM_SETEXTENDEDLISTVIEWSTYLE, LVM_SETITEMTEXTW, LVNI_SELECTED, LVS_EX_DOUBLEBUFFER,
    LVS_EX_FULLROWSELECT, LVS_REPORT, LVS_SHOWSELALWAYS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SendMessageW, WS_CHILD, WS_VISIBLE,
};

pub fn create(parent: HWND, id: i32, x: i32, y: i32, w: i32, h: i32) -> HWND {
    unsafe {
        let lv = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
            w!("SysListView32"),
            PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(
                WS_CHILD.0 | WS_VISIBLE.0 | LVS_REPORT | LVS_SHOWSELALWAYS
                    | windows::Win32::UI::Controls::LVS_OWNERDRAWFIXED,
            ),
            x,
            y,
            w,
            h,
            Some(parent),
            Some(windows::Win32::UI::WindowsAndMessaging::HMENU(id as usize as *mut core::ffi::c_void)),
            Some(super::window::instance()),
            None,
        );
        let lv = lv.unwrap_or_default();
        let ex = (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as usize;
        SendMessageW(
            lv,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            Some(WPARAM(ex)),
            Some(LPARAM(ex as isize)),
        );
        lv
    }
}

pub fn add_column(lv: HWND, index: i32, title: &str, width: i32) {
    let t = wide::wide(title);
    unsafe {
        let mut col = LVCOLUMNW {
            mask: LVCF_FMT | LVCF_WIDTH | LVCF_TEXT | LVCF_SUBITEM,
            fmt: LVCFMT_LEFT,
            cx: width,
            pszText: PWSTR(t.as_ptr() as *mut u16),
            iSubItem: index,
            ..Default::default()
        };
        SendMessageW(
            lv,
            LVM_INSERTCOLUMNW,
            Some(WPARAM(index as usize)),
            Some(LPARAM(&mut col as *mut LVCOLUMNW as isize)),
        );
    }
}

pub fn clear(lv: HWND) {
    unsafe {
        SendMessageW(lv, LVM_DELETEALLITEMS, Some(WPARAM(0)), Some(LPARAM(0)));
    }
}

/// Insert one full row. `id` rides in lParam so the model id survives sorting.
pub fn insert_row(lv: HWND, row: i32, id: u64, cells: &[String]) {
    let first = cells.first().cloned().unwrap_or_default();
    let w0 = wide::wide(&first);
    unsafe {
        let mut item = LVITEMW {
            mask: LVIF_TEXT | LVIF_PARAM,
            iItem: row,
            iSubItem: 0,
            pszText: PWSTR(w0.as_ptr() as *mut u16),
            cchTextMax: 0,
            lParam: LPARAM(id as isize),
            ..Default::default()
        };
        SendMessageW(
            lv,
            LVM_INSERTITEMW,
            Some(WPARAM(0)),
            Some(LPARAM(&mut item as *mut LVITEMW as isize)),
        );
    }
    for (i, cell) in cells.iter().enumerate().skip(1) {
        set_cell(lv, row, i as i32, cell);
    }
}

pub fn set_cell(lv: HWND, row: i32, sub: i32, text: &str) {
    let w = wide::wide(text);
    unsafe {
        let mut item = LVITEMW {
            iSubItem: sub,
            pszText: PWSTR(w.as_ptr() as *mut u16),
            ..Default::default()
        };
        SendMessageW(
            lv,
            LVM_SETITEMTEXTW,
            Some(WPARAM(row as usize)),
            Some(LPARAM(&mut item as *mut LVITEMW as isize)),
        );
    }
}

/// Rows currently selected, in view order.
pub fn selected_rows(lv: HWND) -> Vec<i32> {
    let mut out = Vec::new();
    let mut start: i32 = -1;
    loop {
        let next = unsafe {
            SendMessageW(
                lv,
                LVM_GETNEXTITEM,
                Some(WPARAM(start as usize)),
                Some(LPARAM(LVNI_SELECTED as isize)),
            )
        };
        let idx = next.0 as i32;
        if idx < 0 {
            break;
        }
        out.push(idx);
        start = idx;
    }
    out
}

/// Drop every selection. `LVM_SETITEMSTATE` with index -1 applies to all items;
/// setting a new item selected does not reliably clear the old one.
pub fn clear_selection(lv: HWND) {
    unsafe {
        let mut item = LVITEMW {
            state: windows::Win32::UI::Controls::LIST_VIEW_ITEM_STATE_FLAGS(0),
            stateMask: windows::Win32::UI::Controls::LIST_VIEW_ITEM_STATE_FLAGS(
                windows::Win32::UI::Controls::LVIS_SELECTED.0,
            ),
            ..Default::default()
        };
        SendMessageW(
            lv,
            windows::Win32::UI::Controls::LVM_SETITEMSTATE,
            Some(WPARAM(usize::MAX)),
            Some(LPARAM(&mut item as *mut LVITEMW as isize)),
        );
    }
}

pub fn focus_row(lv: HWND, row: i32) {
    clear_selection(lv);
    unsafe {
        let mut item = LVITEMW {
            state: windows::Win32::UI::Controls::LIST_VIEW_ITEM_STATE_FLAGS(
                windows::Win32::UI::Controls::LVIS_SELECTED.0
                    | windows::Win32::UI::Controls::LVIS_FOCUSED.0,
            ),
            stateMask: windows::Win32::UI::Controls::LIST_VIEW_ITEM_STATE_FLAGS(
                windows::Win32::UI::Controls::LVIS_SELECTED.0
                    | windows::Win32::UI::Controls::LVIS_FOCUSED.0,
            ),
            ..Default::default()
        };
        SendMessageW(
            lv,
            windows::Win32::UI::Controls::LVM_SETITEMSTATE,
            Some(WPARAM(row as usize)),
            Some(LPARAM(&mut item as *mut LVITEMW as isize)),
        );
    }
}

pub fn hit_test(lv: HWND, x: i32, y: i32) -> i32 {
    let mut info = windows::Win32::UI::Controls::LVHITTESTINFO {
        pt: windows::Win32::Foundation::POINT { x, y },
        ..Default::default()
    };
    unsafe {
        SendMessageW(
            lv,
            windows::Win32::UI::Controls::LVM_HITTEST,
            Some(WPARAM(0)),
            Some(LPARAM(&mut info as *mut _ as isize)),
        )
        .0 as i32
    }
}

pub fn set_column_width(lv: HWND, col: i32, width: i32) {
    unsafe {
        SendMessageW(
            lv,
            windows::Win32::UI::Controls::LVM_SETCOLUMNWIDTH,
            Some(WPARAM(col as usize)),
            Some(LPARAM(width as isize)),
        );
    }
}

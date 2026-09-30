//! Small win32 helpers, rescued from the pre-reactive code: resource
//! loaders and word extraction for message params.

use windows::{
    core::*,
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
};

pub fn load_icon(inst: HINSTANCE, name: PCWSTR) -> Result<HICON> {
    match hword(name.0 as isize) {
        0 => unsafe { LoadIconW(None, name) },
        _ => unsafe { LoadIconW(Some(inst), name) },
    }
}

pub fn load_cursor(inst: Option<HINSTANCE>, name: PCWSTR) -> Result<HCURSOR> {
    match inst {
        None => unsafe { LoadCursorW(None, name) },
        Some(inst) => unsafe { LoadCursorW(Some(inst), name) },
    }
}

pub fn get_utf16_vec(text: &str) -> Vec<u16> {
    let mut text: Vec<u16> = text.encode_utf16().collect();
    text.push(0);
    text
}

/// Low word of a message param (control ids, notification codes).
pub fn lword(val: isize) -> i32 {
    (val & 0xffff) as i32
}

/// High word of a message param.
pub fn hword(val: isize) -> i32 {
    ((val >> 16) & 0xffff) as i32
}

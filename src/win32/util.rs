//! Small win32 helpers, rescued from the pre-reactive code: resource
//! loaders and word extraction for message params.

use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::*,
};

use crate::{element::Props, layout::FlexStyle};

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

pub fn calc_scaled<T: Into<f64> + From<f64>>(val: T, dpi: u32) -> T {
    let val = val.into();
    let res = (val * dpi as f64 / 96.).round();
    res.into()
}

pub fn scale_style_props(props: &Props, dpi: u32) -> FlexStyle {
    let mut style = FlexStyle::from_props(props);

    style.gap = calc_scaled(style.gap, dpi);
    style.padding.0 = calc_scaled(style.padding.0, dpi);
    style.padding.1 = calc_scaled(style.padding.1, dpi);
    style.padding.2 = calc_scaled(style.padding.2, dpi);
    style.padding.3 = calc_scaled(style.padding.3, dpi);
    if let Some(width) = style.width {
        style.width = Some(calc_scaled(width, dpi));
    };
    if let Some(height) = style.height {
        style.height = Some(calc_scaled(height, dpi));
    }

    style
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

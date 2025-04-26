use windows::{
    core::*,
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
};

use super::load_cursor;

pub struct WinCreateArgs {
    pub class_name: PCWSTR,
    pub ex_style: WINDOW_EX_STYLE,
    pub style: WINDOW_STYLE,
    pub instance: HINSTANCE,
    pub icon: Option<PCWSTR>,
    pub cursor: HCURSOR,
    pub menu_name: PCWSTR,
    pub window_height: i32,
    pub window_width: i32,
}

impl Default for WinCreateArgs {
    fn default() -> Self {
        WinCreateArgs {
            class_name: w!(""),
            ex_style: WINDOW_EX_STYLE::default(),
            style: WINDOW_STYLE::default(),
            instance: HINSTANCE::default(),
            icon: None,
            cursor: load_cursor(None, IDC_ARROW).unwrap(),
            menu_name: w!(""),
            window_height: CW_USEDEFAULT,
            window_width: CW_USEDEFAULT,
        }
    }
}

impl WinCreateArgs {
    pub fn default_win_main() -> Self {
        WinCreateArgs {
            class_name: w!("mainwin"),
            menu_name: w!("AppMenu"),
            icon: Some(w!("AppIcon")),
            ex_style: WS_EX_APPWINDOW,
            style: WS_OVERLAPPEDWINDOW,
            ..Default::default()
        }
    }
}

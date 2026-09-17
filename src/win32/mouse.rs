use windows::Win32::UI::WindowsAndMessaging::*;

use super::{hword, lword};

#[derive(Debug, PartialEq)]
pub enum MouseEventType {
    Move,
    LeftButtonDown,
    LeftButtonUp,
    LeftButtonDoubleClick,
    RightButtonDown,
    RightButtonUp,
    RightButtonDoubleClick,
    MiddleButtonDown,
    MiddleButtonUp,
    MiddleButtonDoubleClick,
    Unknown,
}

#[derive(Debug)]
pub enum MouseEventMod {
    Control,
    LeftButton,
    MiddleButton,
    RightButton,
    Shift,
    XButton1,
    XButton2,
    None,
    Unknown,
}

#[derive(Debug)]
pub struct MouseEvent {
    pub event_type: MouseEventType,
    pub point: (i32, i32),
    pub opt: MouseEventMod,
}

impl MouseEvent {
    pub fn new(event: &super::Event) -> Self {
        Self {
            event_type: match event.message {
                WM_MOUSEMOVE => MouseEventType::Move,
                WM_LBUTTONDOWN => MouseEventType::LeftButtonDown,
                WM_LBUTTONUP => MouseEventType::LeftButtonUp,
                WM_LBUTTONDBLCLK => MouseEventType::LeftButtonDoubleClick,
                WM_RBUTTONDOWN => MouseEventType::RightButtonDown,
                WM_RBUTTONUP => MouseEventType::RightButtonUp,
                WM_RBUTTONDBLCLK => MouseEventType::RightButtonDoubleClick,
                WM_MBUTTONDOWN => MouseEventType::MiddleButtonDown,
                WM_MBUTTONUP => MouseEventType::MiddleButtonUp,
                WM_MBUTTONDBLCLK => MouseEventType::MiddleButtonDoubleClick,
                _ => MouseEventType::Unknown,
            },
            point: (lword(event.lparam.0), hword(event.lparam.0)),
            opt: match event.wparam.0 {
                0x0008 => MouseEventMod::Control,
                0x0001 => MouseEventMod::LeftButton,
                0x0010 => MouseEventMod::MiddleButton,
                0x0002 => MouseEventMod::RightButton,
                0x0004 => MouseEventMod::Shift,
                0x0020 => MouseEventMod::XButton1,
                0x0040 => MouseEventMod::XButton2,
                0x0000 => MouseEventMod::None,
                _ => MouseEventMod::Unknown,
            },
        }
    }
}

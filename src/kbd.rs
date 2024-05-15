use windows::Win32::UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*};

pub enum KeyboardEventType {
    Keydown,
    Keyup,
    Char,
    Deadchar,
    Unknown,
}

pub struct KbdEvent {
    pub virtual_key: VIRTUAL_KEY,
    pub event_type: KeyboardEventType,
}

impl KbdEvent {
    pub fn new(event: &super::Event) -> Self {
        KbdEvent {
            virtual_key: VIRTUAL_KEY(event.wparam.0 as u16),
            event_type: match event.message {
                WM_KEYDOWN => KeyboardEventType::Keydown,
                WM_KEYUP => KeyboardEventType::Keyup,
                WM_CHAR => KeyboardEventType::Char,
                WM_DEADCHAR => KeyboardEventType::Deadchar,
                _ => KeyboardEventType::Unknown,
            },
        }
    }
}

pub struct Keyboard {}

impl Keyboard {
    pub fn state(vkey: VIRTUAL_KEY) -> bool {
        unsafe { GetKeyState(vkey.0 as i32) > 0 }
    }
}

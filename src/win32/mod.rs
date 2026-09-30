// ---------------------------------------------------------------------------
// The win32 backend — Windows. One backend compiles at a time (target-gated
// deps in Cargo.toml agree). Step-2 scope per the port notes: Window/Div/
// Button/Text through the hand-rolled stack layout engine. Input/Image/List
// land next — their seams are marked in `app.rs`'s mount + WndProc.
// ---------------------------------------------------------------------------

pub mod app;
pub(crate) mod widgets;

mod stack;
mod util;
#[allow(dead_code)]
mod win_create_args;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};

/// The pre-reactive code's wndproc-args abstraction — kept verbatim for the
/// kbd/mouse event parsers, which get wired up when those events land.
#[allow(dead_code)]
#[derive(Debug)]
pub struct Event {
    pub hwnd: HWND,
    pub message: u32,
    pub wparam: WPARAM,
    pub lparam: LPARAM,
}

// parts bin from the pre-reactive code — kept for the Image work later
#[allow(dead_code)]
mod dc;
#[allow(dead_code)]
mod kbd;
#[allow(dead_code)]
mod mouse;

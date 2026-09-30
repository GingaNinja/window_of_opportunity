pub mod component;
pub mod element;
mod layout;
pub mod reconcile;
pub mod state;

// One platform backend compiles at a time (target-gated deps in Cargo.toml
// agree). Backend modules re-export under their historical names so imports
// like `crate::app::AppState` and `window_of_opportunity::app::Application`
// work unchanged on every platform.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::{input, listview, widgets, window};
#[cfg(target_os = "macos")]
pub use macos::app;

#[cfg(target_os = "windows")]
mod win32;
#[cfg(target_os = "windows")]
use win32::widgets;
#[cfg(target_os = "windows")]
pub use win32::app;

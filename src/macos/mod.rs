// ---------------------------------------------------------------------------
// The macOS backend — cacao/AppKit. One backend compiles at a time (target-
// gated deps in Cargo.toml agree). `lib.rs` re-exports these under their
// historical names (`crate::app`, `crate::widgets`, ...) so backend-internal
// and user-facing import paths stay identical across platforms.
// ---------------------------------------------------------------------------

pub mod app;
pub(crate) mod input;
pub(crate) mod listview;
pub(crate) mod widgets;
pub(crate) mod window;

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! The task from "A 2025 Survey of Rust GUI Libraries"
//! (https://www.boringcactus.com/2025/04/13/2025-survey-of-rust-gui-libraries.html),
//! as a MANUAL test bench: a text label and an input field over one piece
//! of state — type in the input, the label follows. That app is the task
//! every library in the post had to build; the post then graded each one
//! BY EYE AND BY EAR on three checks. This example is the bench for
//! repeating those checks yourself (the machine-checkable half lives in
//! `tests/survey.rs`).
//!
//!     cargo run --example survey
//!
//! Expect the post's first screenshot: label and input both "Hello,
//! world!". Then three checks, one per column of the post's table:
//!
//! CHECK 1 — works at all?
//!   Click the input, Ctrl+A, type. The label must mirror the input on
//!   EVERY keystroke — not just on blur. Try backspace, select-and-retype.
//!
//! CHECK 2 — screen reader? (Windows Narrator)
//!   * Win+Ctrl+Enter — Narrator on.
//!   * Tab to the input: Narrator must announce the field AND its
//!     contents ("Hello, world!"). "There is a text input but I can't
//!     read its contents" is the survey's most common failure.
//!   * The label takes no focus (it's a static), so read it in Narrator's
//!     scan mode: Caps Lock + Space, then Down/Up to walk the items —
//!     the label's words must be spoken.
//!   * Win+Ctrl+Enter again — Narrator off.
//!
//! CHECK 3 — IME? (Japanese IME, the post's `toukyou<Tab><Return>`)
//!   One-time setup: Settings → Time & Language → Language & region →
//!   Add a language → Japanese (日本語) — installs Microsoft IME.
//!   * Win+Space — switch to the Japanese IME. Make sure the taskbar
//!     IME indicator says あ (Hiragana), not A (alphanumeric) —
//!     otherwise `toukyou` stays Latin.
//!   * Click the input, Ctrl+A, type `toukyou`.
//!     - provisional とうきょう appears INLINE in the field while typing
//!       (a hidden composer is a partial failure in the table)
//!   * Tab (or Space) — converts to 東京; Return — commits it.
//!   * Field shows 東京. The label must follow to 東京 as well —
//!     that's the converter + state round trip.
//!   * No とうきょう may linger in the field or the label (provisional
//!     text leaking into state is the flutter_rust_bridge failure).
//!   * While typing, watch the field survive: a text field rebuilt under
//!     the IME loses its composition window mid-word.
//!   * Win+Space — switch back.
//!
//! GRADING: one cell per check — works at all? / screen reader? / IME? —
//! plus the IME column's qualifiers ("composer hidden", "converter
//! works", …). Compare with the post's table.

use window_of_opportunity::{app::Application, component::Component, ui};

/// The survey's app, verbatim: label and input over one `text` slot —
/// the same component `tests/survey.rs` drives automatically.
#[derive(Debug, Default)]
struct Survey {}

impl Component for Survey {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let text = ctx.use_state("text", || "Hello, world!".to_string());
        ui! {
            Window width(420.) height(150.) title("survey: label + input") {
                { Div direction("column") gap(6.) padding(8.) {
                    { Text format!("{text}") }
                    { Input value(text) placeholder("type here...")
                            on_change(|state, t| state.update("text", |s: &mut String| *s = t)) }
                } }
            }
        }
    }
}

fn main() {
    let app = Application {};
    app.run(Box::new(Survey {}), |_state, _msg: ()| ());
}

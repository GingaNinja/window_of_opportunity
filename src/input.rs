use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use cacao::input::TextFieldDelegate;

use crate::{app::AppState, state::Handler};

/// The text field's delegate: forwards keystrokes through the on_change
/// handler — the same shape as the window proxy forwarding resizes. The weak
/// back-reference (shared cell, filled in ReactApp::new) lets it run the
/// render loop without owning the app.
pub struct InputDelegate {
    pub app: Rc<RefCell<Option<Weak<RefCell<AppState>>>>>,
    pub on_change: RefCell<Option<Handler>>,
}

impl TextFieldDelegate for InputDelegate {
    const NAME: &'static str = "InputDelegate";

    fn text_did_change(&self, value: &str) {
        let Some(app) = self.app.borrow().as_ref().and_then(Weak::upgrade) else {
            return;
        };
        let Some(Handler::Change(handler)) = self.on_change.borrow().clone() else {
            return;
        };

        // Run it against state — skip if a render is in progress (our own
        // set_text on a fresh field can trigger this synchronously; that
        // render already knows the text).
        if let Ok(app) = app.try_borrow() {
            handler(&app.state, value.to_string());
        } else {
            return;
        }

        // Full remount per keystroke — Phase 1. The field is destroyed here,
        // and positional focus restoration puts typing back together; this
        // is the pain reconciliation (Phase 2) exists to remove.
        app.borrow_mut().render();
    }
}

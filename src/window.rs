use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use cacao::{
    appkit::window::{Window, WindowDelegate},
    core_graphics::display::CGRect,
    objc::{msg_send, runtime::Object, sel, sel_impl},
};

use crate::{
    app::{AppState, TITLEBAR_OFFSET},
    state::Handler,
};

/// The window's delegate: its only job is syncing user resizes into state
/// (window → state), so the next render requests exactly the size the user
/// chose. State → window already flows through the normal render pipeline.
pub struct WindowProxy {
    /// back-reference into the shared app — filled in after AppState exists,
    /// because AppState contains the window (chicken-and-egg). Weak so the
    /// delegate never keeps the app alive.
    pub app: Rc<RefCell<Option<Weak<RefCell<AppState>>>>>,

    /// window handle, provided by did_load
    pub window: RefCell<Option<Window>>,
}

impl WindowDelegate for WindowProxy {
    const NAME: &'static str = "WindowProxy";

    fn did_load(&mut self, window: Window) {
        *self.window.borrow_mut() = Some(window);
    }

    fn did_resize(&self) {
        let Some(app) = self.app.borrow().as_ref().and_then(Weak::upgrade) else {
            return; // app not constructed yet
        };
        let (w, h) = {
            let window = self.window.borrow();
            let Some(window) = window.as_ref() else {
                return;
            };

            // The content view's frame IS the content size; report the *usable*
            // size (content minus the title-bar offset), matching the semantics
            // the width/height props have.
            unsafe {
                let content: *mut Object = msg_send![&*window.objc, contentView];
                let frame: CGRect = msg_send![content, frame];
                (
                    frame.size.width,
                    (frame.size.height - TITLEBAR_OFFSET).max(0.),
                )
            }
        };

        // Take the tree's on_resize handler without holding the app borrow
        // (a render may be in progress — our own set_content_size can fire
        // this callback synchronously mid-render; nothing to do then).
        let handler = app
            .try_borrow()
            .ok()
            .and_then(|app| app.resize_handler.borrow().clone());

        let handler = match handler {
            Some(Handler::Resize(handler)) => handler,
            Some(Handler::Simple(_)) => {
                println!("warning: on_resize expects |state, w, h|");
                return;
            }
            Some(Handler::ListItem(_)) => {
                println!("warning: on_resize expects |state, w, h|");
                return;
            }
            Some(Handler::Change(_)) => {
                println!("warning: on_resize expects |state, w, h|");
                return;
            }
            // No on_resize declared: resizes still stick (the renderer's
            // change-detection won't re-issue an unchanged request), they
            // just don't flow anywhere.
            None => return,
        };

        // Run it against state — slots use interior mutability, so a shared
        // borrow suffices.
        {
            let app = app.borrow();
            handler(&app.state, w, h);
        }

        // and re-render: declaring on_resize is opting into "react to
        // resizes", so the tree updates live during a drag. The size write
        // above converges (request == actual → no-op), so this can't loop.
        // Reload lists with the borrow released — item_for must find it free.
        {
            let mut app = app.borrow_mut();
            app.render();
        }
        if let Ok(app) = app.try_borrow() {
            app.reload_lists();
        }
    }
}

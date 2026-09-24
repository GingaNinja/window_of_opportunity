// ---------------------------------------------------------------------------
// ListView: the macOS list backend
//
// The datasource contract (platform-neutral):
//
//   * at RENDER time, the framework calls the `on_display_item` handler once
//     per row with a real `&Ctx`, and snapshots the resulting element trees
//     into `rows` (see AppState::snapshot_rows)
//   * AppKit pulls rows from this delegate whenever it likes — layout, scroll,
//     window resize — and `item_for` serves them FROM THE SNAPSHOT, never
//     from state: it can fire mid-render, while the state borrow is held
//   * rows are element trees mounted through the same pipeline as the widget
//     tree (AppState::mount_row), into recycled row views (see below)
//
// On win32 the same snapshot is consumed by a WM_DRAWITEM painter instead of
// this delegate — the snapshot is the shared currency between backends.
// ---------------------------------------------------------------------------

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use cacao::{
    listview::{ListView, ListViewDelegate, ListViewRow},
    view::{View, ViewDelegate},
};

use crate::{app::AppState, element::Element, widgets::Widget};

const REACTIVE_ROW: &str = "ReactiveViewRowCell";

/// The list's delegate. AppKit pulls rows from it; it serves the snapshot
/// taken at render time. The app weak back-reference (same pattern as
/// InputDelegate) is for mounting row elements into recycled rows.
pub struct ReactiveListView {
    app: Rc<RefCell<Option<Weak<RefCell<AppState>>>>>,

    /// the row elements the last render produced — the datasource. Stored
    /// pub so the patch pass can refresh the snapshot in place.
    pub rows: RefCell<Vec<Box<Element>>>,

    view: Option<ListView>,
}

impl ReactiveListView {
    pub fn with(
        app: Rc<RefCell<Option<Weak<RefCell<AppState>>>>>,
        rows: Vec<Box<Element>>,
    ) -> Self {
        Self {
            app,
            rows: RefCell::new(rows),
            view: None,
        }
    }
}

impl ListViewDelegate for ReactiveListView {
    const NAME: &'static str = "ReactiveListView";

    fn did_load(&mut self, view: ListView) {
        // a BARE row: no built-in content. A row's content is the element
        // tree mounted by item_for, owned by the row delegate below.
        view.register(REACTIVE_ROW, ReactiveViewRow::default);
        self.view = Some(view);
    }

    fn number_of_items(&self) -> usize {
        self.rows.borrow().len()
    }

    fn item_for(&self, row: usize) -> ListViewRow {
        let mut view = self
            .view
            .as_ref()
            .expect("item_for before the list view loaded")
            .dequeue::<ReactiveViewRow>(REACTIVE_ROW);

        let app = match self.app.borrow().as_ref().and_then(Weak::upgrade) {
            Some(app) => app,
            None => return view.into_row(),
        };
        // Mid-render the app is mutably borrowed and this call came from a
        // synchronous layout pass — return the row unconfigured; the
        // post-render reload pass (AppState::reload_lists) re-queries with
        // the borrow released.
        let app_state = match app.try_borrow() {
            Ok(app) => app,
            Err(_) => return view.into_row(),
        };

        if let Some(element) = self.rows.borrow().get(row).cloned() {
            if let Some(delegate) = view.delegate.as_mut() {
                // Dropping the previous tree unmounts its views (View::Drop
                // removes from superview) — clearing the recycled row.
                // cacao reconstructs this delegate from the view's ivar
                // pointer when the row is recycled, so the same instance
                // (and its view handle) returns for each reuse.
                delegate.content = None;
                delegate.content = Some(app_state.mount_row(&delegate.view, &element));
            }
        }

        view.into_row()
    }
}

/// A bare recycled row: no built-in content. The element tree mounted for
/// whatever row it currently displays is owned here, so recycling drops the
/// old tree (unmounting its subviews) before the next mount — the strip step,
/// for free.
#[derive(Default)]
pub struct ReactiveViewRow {
    /// a handle to the row's view, captured at did_load. Recycled rows don't
    /// get did_load again, but the delegate instance persists across recycles,
    /// so the handle stays valid.
    view: View,

    /// the mounted element tree this row is currently displaying. Dropping
    /// it unmounts the row's content.
    content: Option<Widget>,
}

impl ViewDelegate for ReactiveViewRow {
    const NAME: &'static str = "ReactiveViewRow";

    fn did_load(&mut self, view: View) {
        self.view = view;
    }
}

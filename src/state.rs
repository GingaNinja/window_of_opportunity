// ---------------------------------------------------------------------------
// State: typed, keyed slots (the hooks model)
// ---------------------------------------------------------------------------

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt::{self, Debug},
    rc::Rc,
};

use crate::element::Element;

/// Component state lives in typed slots, keyed by name. Slots survive
/// re-renders and full remounts — that's the whole point: `use_state` reads
/// (or initializes) the same slot every render. Interior mutability means
/// reads (`&Ctx` in render) and writes (events) both go through `&State`.
#[derive(Default)]
pub struct State {
    slots: RefCell<HashMap<String, Rc<RefCell<dyn Any>>>>,
}

impl State {
    /// Reads (or initializes) a slot and returns a clone of its value.
    pub fn use_state<T: Clone + 'static>(&self, key: &str, init: impl FnOnce() -> T) -> T {
        let slot = self
            .slots
            .borrow_mut()
            .entry(key.to_string())
            .or_insert_with(|| Rc::new(RefCell::new(init())))
            .clone();

        let value = slot.borrow();
        match value.downcast_ref::<T>() {
            Some(v) => v.clone(),
            None => panic!("state slot `{key}` holds a different type"),
        }
    }

    /// Applies an updater to a slot.
    pub fn update<T: 'static>(&self, key: &str, updater: impl FnOnce(&mut T)) {
        let slot = self
            .slots
            .borrow()
            .get(key)
            .cloned()
            .unwrap_or_else(|| panic!("no state slot `{key}`"));

        let mut value = slot.borrow_mut();
        match value.downcast_mut::<T>() {
            Some(v) => updater(v),
            None => panic!("state slot `{key}` holds a different type"),
        }
    }
}

/// What a handler prop carries. Different props need different closure
/// shapes, so the map is an enum — more variants (on_change, on_submit,
/// …) slot in here as new widget events appear.
#[derive(Clone)]
pub enum Handler {
    /// Fn(&State) — built by Ctx::set_state, wired by mount_element
    Simple(Event),
    /// Fn(&State, w, h) — wired by the window proxy
    Resize(ResizeHandler),
    /// Fn(&State, String) — wired by the input delegate
    Change(TextChange),
    /// Fn(&Ctx, usize) -> Box<Element> — called ONCE PER ROW AT RENDER TIME
    /// (not at display time): the row elements it returns are snapshotted
    /// into the list delegate, and AppKit serves rows from that snapshot.
    /// See listview.rs — item_for must never touch state directly.
    ListItem(DisplayListRowHandler),
}

impl Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Handler::Simple(_) => f.write_str("<event>"),
            Handler::Resize(_) => f.write_str("<resize-handler>"),
            Handler::Change(_) => f.write_str("<change-handler>"),
            Handler::ListItem(_) => f.write_str("<listItem-handler>"),
        }
    }
}

// #[derive(Clone)]
type TextChange = Rc<dyn Fn(&State, String)>;

// impl TextChange {
//     /// Runs the event against state. Runs on the main thread, from the
//     /// dispatcher.
//     pub fn fire(&self, state: &State, new_text: String) {
//         (self.0)(state, new_text)
//     }
// }

// impl Debug for TextChange {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         f.write_str("<event>")
//     }
// }

/// An on_resize handler: |state, w, h| — runs against state on every
/// resize tick with the new usable size, followed by a re-render.
type ResizeHandler = Rc<dyn Fn(&State, f64, f64)>;

type DisplayListRowHandler = Rc<dyn Fn(&Ctx, usize) -> Box<Element>>;

/// An event: a closure that runs against &State on the main thread when fired,
/// followed by a re-render. Never handed to AppKit directly — see
/// `mount_element` for the id-dispatch trick that keeps the Send boundary
/// happy.
#[derive(Clone)]
pub struct Event(Rc<dyn Fn(&State)>);

impl Event {
    /// Runs the event against state. Runs on the main thread, from the
    /// dispatcher.
    pub fn fire(&self, state: &State) {
        (self.0)(state)
    }
}

impl Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<event>")
    }
}

/// What components render with: read access to state, plus the hooks.
/// Messages that cross onto the main queue — the app's single Send
/// boundary. Widget events travel as dispatch ids; `App(M)` carries the
/// app's own messages (frames, progress, log lines...) from background
/// threads to the GUI. Anything a thread wants to say must fit in here
/// (the same rule as React Native's bridge).
///
/// `M` appears in exactly three places in the framework — this enum, the
/// platform delegate, and `run` — because the widget dispatch path goes
/// through an injected closure (`AppState::dispatch_event`) instead of
/// naming the concrete types.
pub enum Message<M> {
    /// a widget event fired — look up its handler by dispatch id
    Event(usize),
    /// an app message, from anywhere
    App(M),
}

/// Live event registry, by dispatch id — shared bookkeeping, platform
/// neutral. Ids are never reused: a stale id from a previous tree still
/// resolves — and running its updater is harmless, since events address
/// slots by key, not by widget identity. (The map grows by one entry per
/// mounted handler per render — fine for now, prune it when diffing
/// catches up.)
pub struct Handlers {
    by_id: RefCell<HashMap<usize, Handler>>,
    next_id: Cell<usize>,

    /// the root element's on_resize handler, if it declared one — handed to
    /// the window layer so user resizes flow through component logic
    resize: RefCell<Option<Handler>>,
}

impl Default for Handlers {
    fn default() -> Self {
        Self {
            by_id: RefCell::new(HashMap::new()),
            // Dispatch ids start at 1. On win32 a dispatch id is also the
            // control id, and 0 is reserved there: it's the NULL HMENU and
            // the "this control has no handler" sentinel — starting above
            // it keeps the two spaces from colliding (a bug once observed as
            // "clicking either button runs the one handler").
            next_id: Cell::new(1),
            resize: RefCell::new(None),
        }
    }
}

impl Handlers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores an event under a fresh dispatch id and returns it. Ids are
    /// dense and increasing, never 0 (see `Default`).
    pub fn register(&self, event: &Handler) -> usize {
        let id = self.next_id.replace(self.next_id.get() + 1);
        self.by_id.borrow_mut().insert(id, event.clone());
        id
    }

    /// Re-stores the handler under an existing dispatch id — the widget's
    /// action closure keeps firing this id while the handler stays current.
    pub fn refresh(&self, id: usize, event: &Handler) {
        self.by_id.borrow_mut().insert(id, event.clone());
    }

    /// The live handler for a dispatch id, if it's still around.
    pub fn event(&self, id: usize) -> Option<Handler> {
        self.by_id.borrow().get(&id).cloned()
    }

    /// Fires the handler for a dispatch id — false if the id is stale.
    pub fn fire_simple(&self, id: usize, state: &State) -> bool {
        match self.event(id) {
            Some(Handler::Simple(event)) => {
                event.fire(state);
                true
            }
            _ => false,
        }
    }

    pub fn fire_text_change(&self, id: usize, state: &State, new_text: String) -> bool {
        match self.event(id) {
            Some(Handler::Change(event)) => {
                event(state, new_text);
                true
            }
            _ => false,
        }
    }

    pub fn set_resize_handler(&self, handler: Option<Handler>) {
        *self.resize.borrow_mut() = handler;
    }

    pub fn resize_handler(&self) -> Option<Handler> {
        self.resize.borrow().clone()
    }
}

pub struct Ctx<'a> {
    pub state: &'a State,
}

impl Ctx<'_> {
    /// useState: read (or initialize) a typed slot. The key is global, so
    /// give distinct components distinct keys (React keys state by tree
    /// position instead — a possible future refinement).
    pub fn use_state<T: Clone + 'static>(&self, key: &str, init: impl FnOnce() -> T) -> T {
        self.state.use_state(key, init)
    }

    /// Builds an Event that applies `updater` to the slot when fired — the
    /// React `setCount(c => c + 1)` shape, ready to hand to on_click(...).
    /// The event primitive set_state is built on: arbitrary logic against
    /// &State when the event fires.
    pub fn effect(&self, f: impl Fn(&State) + 'static) -> Event {
        Event(Rc::new(f))
    }

    pub fn set_state<T: 'static>(&self, key: &str, updater: impl Fn(&mut T) + 'static) -> Event {
        let key = key.to_string();
        Event(Rc::new(move |state: &State| {
            state.update(&key, |v| updater(v))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handlers_register_refresh_fire() {
        let state = State::default();
        let handlers = Handlers::new();

        let fired = Rc::new(RefCell::new(0));
        let f = fired.clone();
        let event = Handler::Simple(Event(Rc::new(move |_| *f.borrow_mut() += 1)));

        let id = handlers.register(&event);
        assert!(handlers.fire_simple(id, &state));
        assert_eq!(*fired.borrow(), 1);

        // refresh keeps the id stable but the handler current
        let f = fired.clone();
        let fresh = Handler::Simple(Event(Rc::new(move |_| *f.borrow_mut() += 10)));
        handlers.refresh(id, &fresh);
        assert!(handlers.fire_simple(id, &state));
        assert_eq!(*fired.borrow(), 11, "the refreshed handler ran");

        // stale id is refused, not panicked
        assert!(!handlers.fire_simple(999, &state));

        // the root's on_resize rides along in the same bookkeeping
        assert!(handlers.resize_handler().is_none());
        handlers.set_resize_handler(Some(event));
        assert!(matches!(
            handlers.resize_handler(),
            Some(Handler::Simple(_))
        ));
    }

    #[test]
    fn dispatch_ids_never_collide_with_the_no_handler_sentinel() {
        let handlers = Handlers::new();
        let event = Handler::Simple(Event(Rc::new(|_| {})));

        let first = handlers.register(&event);
        let second = handlers.register(&event);
        assert_ne!(
            first, 0,
            "win32 control id 0 means 'no handler' — dispatch ids start above it"
        );
        assert_eq!(second, first + 1, "ids are dense and increasing");
    }
}

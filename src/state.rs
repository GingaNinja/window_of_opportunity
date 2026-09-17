// ---------------------------------------------------------------------------
// State: typed, keyed slots (the hooks model)
// ---------------------------------------------------------------------------

use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    fmt::{self, Debug},
    rc::Rc,
};

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
    Change(Rc<dyn Fn(&State, String)>),
}

impl Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Handler::Simple(_) => f.write_str("<event>"),
            Handler::Resize(_) => f.write_str("<resize-handler>"),
            Handler::Change(_) => f.write_str("<change-handler>"),
        }
    }
}

/// An on_resize handler: |state, w, h| — runs against state on every
/// resize tick with the new usable size, followed by a re-render.
type ResizeHandler = Rc<dyn Fn(&State, f64, f64)>;

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

// ---------------------------------------------------------------------------
// The win32 Widget enum — HWND-backed, the cfg-parallel of the cacao one.
// Containers are layout-only regions (their controls are children of the
// main HWND): enough for Window/Div/Button/Text; a Div background arrives
// with WM_CTLCOLOR* handling later.
// ---------------------------------------------------------------------------

use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::DestroyWindow};

use crate::{element::Element, reconcile::WidgetKind};

pub enum Widget {
    /// Window/Div: a region in the parent's coordinate space. Children are
    /// positioned by the stack layout engine.
    Container { children: Vec<Widget> },
    Button {
        hwnd: HWND,
        /// the dispatch id of the wired on_click — also this control's child
        /// id (what WM_COMMAND carries back), stable across re-renders like
        /// the cacao side
        handler_id: Option<usize>,
    },
    Label { hwnd: HWND },
}

/// Which neutral widget flavor is this — the platform half of the
/// compatibility table (the table itself lives in `reconcile`).
impl Widget {
    pub fn kind(&self) -> WidgetKind {
        match self {
            Widget::Container { .. } => WidgetKind::Container,
            Widget::Button { .. } => WidgetKind::Button,
            Widget::Label { .. } => WidgetKind::Label,
        }
    }
}

/// Mirrors the cacao side: dropping a widget unmounts it. Leaf HWNDs
/// destroy themselves here; containers are regions whose children drop
/// recursively.
impl Drop for Widget {
    fn drop(&mut self) {
        if let Widget::Button { hwnd, .. } | Widget::Label { hwnd } = self {
            unsafe {
                let _ = DestroyWindow(*hwnd);
            }
        }
    }
}

/// Can this widget represent the new element in the same position? Thin
/// wrapper over the neutral table in `reconcile`.
pub fn compatible(widget: &Widget, new_el: &Element) -> bool {
    crate::reconcile::compatible(widget.kind(), &new_el.element_type)
}

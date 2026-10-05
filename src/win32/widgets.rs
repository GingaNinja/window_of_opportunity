// ---------------------------------------------------------------------------
// The win32 Widget enum — HWND-backed, the cfg-parallel of the cacao one.
// Containers are layout-only regions (their controls are children of the
// main HWND): enough for Window/Div/Button/Text; a Div background arrives
// with WM_CTLCOLOR* handling later.
// ---------------------------------------------------------------------------

use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{DeleteObject, HBRUSH},
    UI::WindowsAndMessaging::{DestroyWindow, GetParent},
};

use crate::{
    element::Element,
    reconcile::WidgetKind,
    state::{Event, Handler},
};

pub enum Widget {
    /// Window/Div: a real child window (the `wo_div` class) — it paints its
    /// own background and is the parent of its children's controls.
    /// `hwnd` for the ROOT container is the main window itself.
    Container {
        hwnd: HWND,
        /// the background brush, if the element asked for one — `None`
        /// means "inherit the parent's", like a transparent NSView
        background: Option<HBRUSH>,
        children: Vec<Widget>,
    },
    Button {
        hwnd: HWND,
        /// the on_click handler, OWNED by the widget (the InputDelegate
        /// model) — WM_COMMAND's lparam is this hwnd, so the event source
        /// resolves itself; no dispatch ids on win32
        on_click: Option<Event>,
    },
    Input {
        hwnd: HWND,
        /// The on_change handler, OWNED by the widget — the InputDelegate
        /// model. Payload-carrying events live with their widget (the text
        /// comes from the control at fire time); the id registry is for
        /// payload-free dispatch.
        on_change: Option<Handler>,
    },
    Label {
        hwnd: HWND,
    },
    /// A virtual list-view (LVS_OWNERDATA): the control holds only a COUNT;
    /// `rows` is the render-time snapshot it serves to visible rows — the
    /// twin of the macOS delegate's snapshot.
    List {
        hwnd: HWND,
        rows: Vec<Box<Element>>,
    },
}

/// Which neutral widget flavor is this — the platform half of the
/// compatibility table (the table itself lives in `reconcile`).
impl Widget {
    pub fn kind(&self) -> WidgetKind {
        match self {
            Widget::Container { .. } => WidgetKind::Container,
            Widget::Button { .. } => WidgetKind::Button,
            Widget::Label { .. } => WidgetKind::Label,
            Widget::Input { .. } => WidgetKind::Input,
            Widget::List { .. } => WidgetKind::List,
        }
    }
}

/// Mirrors the cacao side: dropping a widget unmounts it. Leaf HWNDs
/// destroy themselves here; a container destroys its div window (which
/// takes its children's controls with it — their own Drops then no-op on
/// the already-gone handles) and frees its brush. The root container is
/// skipped: its hwnd IS the window the user owns.
impl Drop for Widget {
    fn drop(&mut self) {
        match self {
            Widget::Button { hwnd, .. }
            | Widget::Label { hwnd }
            | Widget::Input { hwnd, .. }
            | Widget::List { hwnd, .. } => unsafe {
                let _ = DestroyWindow(*hwnd);
            },
            Widget::Container {
                hwnd, background, ..
            } => unsafe {
                if GetParent(*hwnd).is_ok() {
                    let _ = DestroyWindow(*hwnd);
                }
                if let Some(brush) = background {
                    let _ = DeleteObject((*brush).into());
                }
            },
        }
    }
}

/// Can this widget represent the new element in the same position? Thin
/// wrapper over the neutral table in `reconcile`.
pub fn compatible(widget: &Widget, new_el: &Element) -> bool {
    crate::reconcile::compatible(widget.kind(), &new_el.element_type)
}

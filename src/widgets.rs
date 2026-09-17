// ---------------------------------------------------------------------------
// Widget tree (the mounted counterpart of the Element tree)
// ---------------------------------------------------------------------------

use cacao::{
    button::Button,
    image::ImageView,
    input::TextField,
    layout::{LayoutAnchorDimension, LayoutAnchorX, LayoutAnchorY, LayoutConstraint},
    text::Label,
    view::View,
};

use crate::{
    element::{Element, ElementType},
    input::InputDelegate,
};

pub enum Widget {
    Container {
        view: View,
        children: Vec<Widget>,
        /// the AutoLayout constraints this container generated for itself
        /// and its direct children — kept so patching can deactivate exactly
        /// what it regenerates
        constraints: Vec<LayoutConstraint>,
    },
    Button {
        control: Button,
        /// the dispatch id of the wired on_click — stable across re-renders:
        /// the button's action closure keeps firing this id while patching
        /// refreshes the handler stored under it (no unbounded growth)
        handler_id: Option<usize>,
    },
    Label(Label),
    Input(TextField<InputDelegate>),
    ImageView {
        view: ImageView,
        /// the trace frame version this view is currently displaying — so
        /// patching only re-blits when a new frame actually arrived
        version: u64,
    },
}

/// Every cacao control exposes the same layout anchors; this gives us uniform
/// access no matter which widget type a child turned out to be.
pub struct Anchors {
    pub top: LayoutAnchorY,
    pub bottom: LayoutAnchorY,
    pub leading: LayoutAnchorX,
    pub trailing: LayoutAnchorX,
    pub width: LayoutAnchorDimension,
    pub height: LayoutAnchorDimension,
}

impl Widget {
    pub fn anchors(&self) -> Anchors {
        match self {
            Widget::Container { view, .. } => Anchors {
                top: view.top.clone(),
                bottom: view.bottom.clone(),
                leading: view.leading.clone(),
                trailing: view.trailing.clone(),
                width: view.width.clone(),
                height: view.height.clone(),
            },
            Widget::Button { control, .. } => Anchors {
                top: control.top.clone(),
                bottom: control.bottom.clone(),
                leading: control.leading.clone(),
                trailing: control.trailing.clone(),
                width: control.width.clone(),
                height: control.height.clone(),
            },
            Widget::Label(label) => Anchors {
                top: label.top.clone(),
                bottom: label.bottom.clone(),
                leading: label.leading.clone(),
                trailing: label.trailing.clone(),
                width: label.width.clone(),
                height: label.height.clone(),
            },
            Widget::Input(field) => Anchors {
                top: field.top.clone(),
                bottom: field.bottom.clone(),
                leading: field.leading.clone(),
                trailing: field.trailing.clone(),
                width: field.width.clone(),
                height: field.height.clone(),
            },
            Widget::ImageView { view, .. } => Anchors {
                top: view.top.clone(),
                bottom: view.bottom.clone(),
                leading: view.leading.clone(),
                trailing: view.trailing.clone(),
                width: view.width.clone(),
                height: view.height.clone(),
            },
        }
    }
}

/// The props that feed constraint generation — compared between old and new
/// elements to decide whether a container (or its parent) needs relayout.
const FLEX_PROPS: &[&str] = &["direction", "gap", "padding", "width", "height", "grow"];

pub fn flex_changed(old: &Element, new: &Element) -> bool {
    FLEX_PROPS
        .iter()
        .any(|key| old.props.get(*key) != new.props.get(*key))
}

/// Can this widget represent the new element in the same position? Kind
/// must match kind. (Positional only — reordering is not detected; that's
/// what keys are for, in a future pass.)
pub fn compatible(widget: &Widget, new_el: &Element) -> bool {
    matches!(
        (widget, &new_el.element_type),
        (
            Widget::Container { .. },
            ElementType::Window | ElementType::Div
        ) | (Widget::Button { .. }, ElementType::Button)
            | (Widget::Label(_), ElementType::Text(_))
            | (Widget::Input(_), ElementType::Input)
            | (Widget::ImageView { .. }, ElementType::Image)
    )
}

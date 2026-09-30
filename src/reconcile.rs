// ---------------------------------------------------------------------------
// The reconciliation driver — platform-neutral.
//
// Everything here operates on Element trees and plain data: expanding
// components, diffing children, deciding widget reuse. Platform backends
// plug in at the seams — `Widget::kind()` for the compatibility table, and
// mount/patch/layout closures in `reconcile_children`. Shared by the macOS
// backend today, the win32 backend tomorrow: keep this file free of
// cacao/win32 types.
// ---------------------------------------------------------------------------

use crate::{
    element::{Element, ElementType, PropType},
    state::{Ctx, Handler, State},
};

/// Platform-neutral widget flavor — the left-hand side of the compatibility
/// table. Each backend maps its concrete widget type onto these kinds via
/// its `kind()` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    Container,
    Button,
    Label,
    Input,
    Image,
    List,
}

/// The props that feed constraint generation — compared between old and new
/// elements to decide whether a container (or its parent) needs relayout.
const FLEX_PROPS: &[PropType] = &[
    PropType::Direction,
    PropType::Gap,
    PropType::Padding,
    PropType::Width,
    PropType::Height,
    PropType::Grow,
];

pub fn flex_changed(old: &Element, new: &Element) -> bool {
    FLEX_PROPS
        .iter()
        .any(|key| old.props.get(*key) != new.props.get(*key))
}

/// Can a widget of this kind represent the new element in the same position?
/// Kind must match kind. (Positional only — reordering is not detected;
/// that's what keys are for, in a future pass.)
pub fn compatible(kind: WidgetKind, new_el: &ElementType) -> bool {
    matches!(
        (kind, new_el),
        (WidgetKind::Container, ElementType::Window | ElementType::Div)
            | (WidgetKind::Button, ElementType::Button)
            | (WidgetKind::Label, ElementType::Text(_))
            | (WidgetKind::Input, ElementType::Input)
            | (WidgetKind::Image, ElementType::Image)
            | (WidgetKind::List, ElementType::List)
    )
}

/// Expand component nodes with the current state, so patch/mount only ever
/// see primitives. (List rows are the one pipeline entry that bypasses the
/// tree's expand pass — they expand at snapshot time, see `snapshot_rows`.)
pub fn expand(state: &State, el: &Element) -> Box<Element> {
    match &el.element_type {
        ElementType::Component(component) => {
            let children = el
                .children
                .iter()
                .map(|child| expand(state, child))
                .collect();
            let ctx = Ctx { state };
            let rendered = component.render(&ctx, children);
            expand(state, &rendered)
        }
        _ => Box::new(Element {
            element_type: el.element_type.clone(),
            props: el.props.clone(),
            handlers: el.handlers.clone(),
            children: el
                .children
                .iter()
                .map(|child| expand(state, child))
                .collect(),
        }),
    }
}

/// Builds the row elements for a List element against current state — the
/// snapshot the list delegate serves from in `item_for`. Runs once per
/// render (mount or patch), never at display time. Count comes from the
/// `rows(n)` prop; the handler from `on_display_item`.
pub fn snapshot_rows(state: &State, el: &Element) -> Vec<Box<Element>> {
    let count: usize = el.props.get_usize(PropType::Rows).unwrap_or_default();
    let ctx = Ctx { state };
    match el.handlers.get("on_display_item") {
        Some(Handler::ListItem(handler)) => (0..count)
            // Rows pass through `expand` here — component nodes must be
            // inlined before mount. Rows are the one pipeline entry that
            // bypasses the tree's expand pass, so it happens now.
            .map(|i| expand(state, &handler(&ctx, i)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The children-diff skeleton, platform-neutral: positional matching,
/// replace-on-incompatibility, append, truncate. Returns whether anything
/// changed that requires relayout (count change, replacement, or a flex
/// prop change on a child). The platform supplies what "mount", "patch" and
/// "compatible" mean for its widget type.
pub fn reconcile_children<W>(
    widgets: &mut Vec<W>,
    old_children: &[Box<Element>],
    new_children: &[Box<Element>],
    mut compatible_child: impl FnMut(&W, &Element) -> bool,
    mut patch_child: impl FnMut(&mut W, &Element, &Element),
    mut mount_child: impl FnMut(&Element) -> W,
) -> bool {
    let mut changed = old_children.len() != new_children.len();

    for index in 0..new_children.len() {
        let new_child = &new_children[index];

        let mut slot_ok = false;
        if let (Some(slot), Some(old_child)) = (widgets.get_mut(index), old_children.get(index)) {
            slot_ok = compatible_child(slot, new_child);
            if slot_ok {
                patch_child(slot, old_child, new_child);
                if flex_changed(old_child, new_child) {
                    changed = true;
                }
            }
        }

        if !slot_ok {
            // replaced (old drops) or appended
            let fresh = mount_child(new_child);
            match widgets.get_mut(index) {
                Some(slot) => *slot = fresh,
                None => widgets.push(fresh),
            }
            changed = true;
        }
    }

    // vanished children drop — the platform's widget drop unmounts them
    widgets.truncate(new_children.len());

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Prop, Props};
    use std::collections::HashMap;

    fn text(s: &str) -> Box<Element> {
        Box::new(Element {
            element_type: ElementType::Text(s.to_string()),
            props: Props::new(),
            handlers: HashMap::new(),
            children: vec![],
        })
    }

    fn text_of(el: &Element) -> &str {
        match &el.element_type {
            ElementType::Text(t) => t,
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn compatibility_table() {
        use WidgetKind::*;
        assert!(compatible(Container, &ElementType::Div));
        assert!(compatible(Container, &ElementType::Window));
        assert!(compatible(Button, &ElementType::Button));
        assert!(compatible(Label, &ElementType::Text("x".into())));
        assert!(compatible(Input, &ElementType::Input));
        assert!(compatible(Image, &ElementType::Image));
        assert!(compatible(List, &ElementType::List));

        assert!(!compatible(Button, &ElementType::Div));
        assert!(!compatible(Container, &ElementType::Text("x".into())));
    }

    #[test]
    fn flex_props_drive_the_relayout_flag() {
        let width = |w: f64| {
            let mut props = Props::new();
            props.insert(PropType::Width, Prop::Float(w));
            Box::new(Element {
                element_type: ElementType::Div,
                props,
                handlers: HashMap::new(),
                children: vec![],
            })
        };
        assert!(!flex_changed(&width(2.), &width(2.)));
        assert!(flex_changed(&width(2.), &width(3.)));
    }

    /// Toy widget: a string mirroring the element's text, so compatibility
    /// is "same text" and the skeleton's calls are easy to count.
    #[test]
    fn reconcile_replaces_appends_and_truncates() {
        let mut widgets = vec!["one".to_string(), "gone".to_string()];
        let old = vec![text("one"), text("gone")];
        let new = vec![text("one"), text("two"), text("three")];

        let mut patches = 0;
        let mut mounts = 0;
        let changed = reconcile_children(
            &mut widgets,
            &old,
            &new,
            |w, el| w == text_of(el),
            |_, _, _| patches += 1,
            |el| {
                mounts += 1;
                text_of(el).to_string()
            },
        );

        assert!(changed, "replacement + append require relayout");
        assert_eq!(patches, 1, "\"one\" is compatible and reused");
        assert_eq!(mounts, 2, "\"two\" replaces \"gone\", \"three\" appends");
        assert_eq!(widgets, vec!["one", "two", "three"]);
    }

    #[test]
    fn reconcile_drops_vanished_children() {
        let mut widgets = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let old = vec![text("a"), text("b"), text("c")];
        let new = vec![text("a")];

        let changed = reconcile_children(
            &mut widgets,
            &old,
            &new,
            |w, el| w == text_of(el),
            |_, _, _| {},
            |el| text_of(el).to_string(),
        );

        assert!(changed);
        assert_eq!(widgets, vec!["a"]);
    }

    #[test]
    fn reconcile_reports_child_flex_changes() {
        let width = |s: &str, w: f64| {
            let mut el = text(s);
            el.props.insert(PropType::Width, Prop::Float(w));
            el
        };
        let mut widgets = vec!["same".to_string()];
        let old = vec![width("same", 2.)];
        let new = vec![width("same", 3.)]; // same kind, but flex props differ

        let changed = reconcile_children(
            &mut widgets,
            &old,
            &new,
            |w, el| w == text_of(el),
            |_, _, _| {},
            |el| text_of(el).to_string(),
        );
        assert!(changed, "a child's flex change needs relayout");
    }
}

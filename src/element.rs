use std::{collections::HashMap, rc::Rc, sync::Arc};

use crate::{component::Component, state::Handler};

#[derive(Debug, Clone)]
pub struct Element {
    pub element_type: ElementType,
    pub props: HashMap<String, String>,
    /// events attached to this element, by prop name (on_click, on_resize).
    /// Rc-cloned into the tree each render — cheap, and stale copies are
    /// harmless (they address state slots by key).
    pub handlers: HashMap<String, Handler>,
    pub children: Vec<Box<Element>>,
}

#[derive(Debug, Clone)]
pub enum ElementType {
    Window,
    Button,
    Div,
    Text(String),
    Input,
    Image,
    List,
    /// A component invocation, as a tree node — expanded by the framework
    /// during render with access to state. (Rc so the enum stays Clone-able.)
    Component(Rc<dyn Component>),
}

/// Window-level props, parsed from the tree root (the Window element).
/// Parsed once before the window exists — style masks (resizability) can only be
/// set at creation — and again on every render, since sizing is live.
pub struct WindowSpec {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub resizable: bool,
    pub title: String,
}

pub fn window_spec(tree: &Element) -> WindowSpec {
    let num = |key: &str| tree.props.get(key).and_then(|value| value.parse().ok());

    WindowSpec {
        width: num("width"),
        height: num("height"),
        title: tree.props.get("title").unwrap_or(&"".to_string()).clone(),
        resizable: tree
            .props
            .get("resizable")
            .map(|v| v != "false")
            .unwrap_or(true),
    }
}

#[macro_export]
macro_rules! ui {
    (Window $($val:tt) *) => {
        ui! { @element Window $($val)* }
    };
    (Div $($val:tt) *) => {
        ui! { @element Div $($val)* }
    };
    (Button $($val:tt) *) => {
        ui! { @element Button $($val)* }
    };
    (Input $($val:tt) *) => {
        ui! { @element Input $($val)* }
    };
    (Image $($val:tt) *) => {
        ui! { @element Image $($val)* }
    };
    (List $($val:tt) *) => {
        ui! { @element List $($val)* }
    };
    // Text with props — content is BRACED, the same convention as children
    // on the other elements: Text color("blue") font_size(10) { content }
    // The braces are load-bearing: without them a call-shaped content
    // (helper(x), item.clone()) is indistinguishable from one more prop at
    // the ident/expr boundary, and rustc raises a local-ambiguity error.
    // Props are RECORDED via @prop — same machinery as every other element
    // — so they're available to rendering later; whether a label honors
    // background/color is a separate, rendering-side task.
    (Text $($prop:ident ($($val:tt)*))* { $contents:expr }) => {
        {
            use std::collections::HashMap;
            let mut el = $crate::element::Element {
                element_type: $crate::element::ElementType::Text($contents.into()),
                props: HashMap::new(),
                handlers: HashMap::new(),
                children: vec![],
            };
            $( ui!(@prop el, $prop ($($val)*)); )*
            Box::new(el)
        }
    };
    (Text $contents:expr) => {
        {
            use std::collections::HashMap;
            Box::new($crate::element::Element {
                element_type: $crate::element::ElementType::Text($contents.into()),
                props: HashMap::new(),
                handlers: HashMap::new(),
                children: vec![],
            })
        }
    };
    (CHILDREN $expr:expr) => {
        $expr
    };
    // A bare component invocation: a component with no children.
    ($comp:ident) => {
        {
            use std::collections::HashMap;
            // A component invocation stays as a tree node — the framework
            // expands it with current state during render (see `expand`).
            // (Macro hygiene: the macro can't reference the caller's `state`
            // directly, so expansion happens outside the macro.)
            Box::new($crate::element::Element {
                element_type: $crate::element::ElementType::Component(std::rc::Rc::new($comp {})),
                props: HashMap::new(),
                handlers: HashMap::new(),
                children: vec![],
            })
        }
    };
    // A component invocation with children: TodoView { { Text "hi" } }
    ($comp:ident { $($inner:tt)* }) => {
        {
            use std::collections::HashMap;
            let mut el = $crate::element::Element {
                element_type: $crate::element::ElementType::Component(std::rc::Rc::new($comp {})),
                props: HashMap::new(),
                handlers: HashMap::new(),
                children: vec![],
            };
            ui!(@children el, $($inner)*);
            Box::new(el)
        }
    };
    // element with props and a braced child list:
    //   Div direction("row") gap(10.) { ... }
    (@element $el:ident $($prop:ident ($($val:tt)*))* { $($children:tt)* }) => {
        {
            use std::collections::HashMap;
            let mut el = $crate::element::Element {
                element_type: $crate::element::ElementType::$el,
                props: HashMap::new(),
                handlers: HashMap::new(),
                children: vec![],
            };
            $( ui!(@prop el, $prop ($($val)*)); )*
            ui!(@children el, $($children)*);
            Box::new(el)
        }
    };
    // element with props, no children: Div height(80.) background("blue")
    (@element $el:ident $($prop:ident ($($val:tt)*))*) => {
        {
            use std::collections::HashMap;
            let mut el = $crate::element::Element {
                element_type: $crate::element::ElementType::$el,
                props: HashMap::new(),
                handlers: HashMap::new(),
                children: vec![],
            };
            $( ui!(@prop el, $prop ($($val)*)); )*
            Box::new(el)
        }
    };
    // ---- prop parsing ----
    // Handler props hold handler closures; everything else stringifies
    // its runtime value into `props`.
    (@prop $el:ident, on_click($($val:tt)*)) => {
        $el.handlers.insert("on_click".to_string(), $crate::state::Handler::Simple(($($val)*)));
    };
    // on_resize takes |state, w, h| — it runs on every resize tick
    (@prop $el:ident, on_resize($($val:tt)*)) => {
        $el.handlers
            .insert("on_resize".to_string(), $crate::state::Handler::Resize(std::rc::Rc::new(($($val)*))));
    };
    // on_change takes |state, text| — it fires on each keystroke
    (@prop $el:ident, on_change($($val:tt)*)) => {
        $el.handlers
            .insert("on_change".to_string(), $crate::state::Handler::Change(std::rc::Rc::new(($($val)*))));
    };
    (@prop $el:ident, on_display_item($($val:tt)*)) => {
        $el.handlers
            .insert("on_display_item".to_string(), $crate::state::Handler::ListItem(std::rc::Rc::new(($($val)*))));
    };
    (@prop $el:ident, $prop:ident($($val:tt)*)) => {
        // Evaluate the value at tree-build time: literal props and computed
        // expressions (width(w)) both stringify their runtime value.
        $el.props
            .insert(stringify!($prop).to_string(), format!("{}", ($($val)*)));
    };
    // ---- child-list parsing ----
    // macro_rules can't know where one child ends and the next begins when
    // children are multi-token (`Div height(80.) {}` is 3+ tts), so child
    // elements must be wrapped in braces (one group = one child). Bare `Text`
    // literals, `CHILDREN` splices, and bare components with children are
    // recognized without braces.
    (@children $v:ident,) => {};
    (@children $v:ident, Text $text:literal $($rest:tt)*) => {
        $v.children.push(ui!(Text $text));
        ui!(@children $v, $($rest)*);
    };
    (@children $v:ident, { $($inner:tt)* } $($rest:tt)*) => {
        $v.children.push(ui! { $($inner)* });
        ui!(@children $v, $($rest)*);
    };
    (@children $v:ident, CHILDREN $splice:ident $($rest:tt)*) => {
        $v.children.push($splice);
        ui!(@children $v, $($rest)*);
    };
    (@children $v:ident, $comp:ident { $($inner:tt)* } $($rest:tt)*) => {
        $v.children.push(ui!($comp { $($inner)* }));
        ui!(@children $v, $($rest)*);
    };
    // ---- expression children ----
    // LAST arm, on purpose: anything that isn't an element, Text, the
    // CHILDREN splice, a component invocation, or an internal @-rule is an
    // expression evaluating to Box<Element>. So a match, an if, a function
    // call, or a parenthesized prebuilt element works directly as a child:
    //   { match flag { A => ui!{ Text "a" }, B => ui!{ Text "b" } } }
    // (A *bare* identifier stays a component invocation — splice a binding
    // instead with { CHILDREN my_element }.)
    ($expr:expr) => {
        {
            // the annotation turns "expected struct Element, found i32" into
            // an error pointing at the child expression itself
            let child: Box<$crate::element::Element> = $expr;
            child
        }
    };
}

/// Somewhere to hold some pixel data, to then add as src on an Image.
/// (`src` on the Image element names the slot). `version` ticks per frame so
/// the image widget can skip re-blitting unchanged data.
#[derive(Clone)]
pub struct BlitFrame {
    pub width: usize,
    pub height: usize,
    pub samples: u64,
    pub pixels: Arc<Vec<u8>>,
    pub version: u64,
}

impl BlitFrame {
    pub fn empty() -> Self {
        BlitFrame {
            width: 0,
            height: 0,
            samples: 0,
            pixels: Arc::new(Vec::new()),
            version: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn helper(_x: usize) -> String {
        "helper!".to_string()
    }

    #[test]
    fn text_props_are_recorded_and_content_survives() {
        let el = crate::ui! { Text background("blue") { "Complete".to_string() } };
        assert_eq!(
            el.props.get("background").map(String::as_str),
            Some("blue"),
            "prop must land in el.props"
        );
        assert!(
            matches!(&el.element_type, ElementType::Text(t) if t == "Complete"),
            "content must survive: {:?}",
            el.element_type
        );
    }

    #[test]
    fn text_plain_content_forms() {
        // literal, field access, macro call, plain call expression — the call
        // form is the ambiguous one vs. prop syntax, and must read as content
        let el = crate::ui! { Text "plain" };
        assert!(matches!(&el.element_type, ElementType::Text(t) if t == "plain"));

        let el = crate::ui! { Text helper(3) };
        assert!(matches!(&el.element_type, ElementType::Text(t) if t == "helper!"));

        let el = crate::ui! { Text format!("number: {}", 3) };
        assert!(matches!(&el.element_type, ElementType::Text(t) if t == "number: 3"));

        let item = String::from("field");
        let el = crate::ui! { Text item.clone() };
        assert!(matches!(&el.element_type, ElementType::Text(t) if t == "field"));
        assert!(
            el.props.is_empty(),
            "no props on plain content: {:?}",
            el.props
        );

        // multiple props + content: content goes in braces (see the macro
        // arm — unbraced content after props is ambiguous and won't compile)
        let el = crate::ui! { Text prop1(1) prop2(2) { item.clone() } };
        assert!(matches!(&el.element_type, ElementType::Text(t) if t == "field"));
        assert_eq!(el.props.get("prop1").map(String::as_str), Some("1"));
        assert_eq!(el.props.get("prop2").map(String::as_str), Some("2"));
    }
}

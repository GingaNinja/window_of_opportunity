use std::{collections::HashMap, rc::Rc, sync::Arc};

use crate::{component::Component, state::Handler};

#[derive(Debug, Clone)]
pub struct Element {
    pub element_type: ElementType,
    pub props: Props,
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
    WindowSpec {
        width: tree.props.get_float(PropType::Width),
        height: tree.props.get_float(PropType::Height),
        title: tree
            .props
            .get_string(PropType::Title)
            .unwrap_or_default()
            .to_string(),
        resizable: tree.props.get_bool(PropType::Resizable).unwrap_or(true),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    Float(f64),
    Usize(usize),
    String(String),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum PropType {
    Width,
    Height,
    Title,
    Resizable,
    Background,
    Rows,
    Color,
    FontSize,
    Value,
    Placeholder,
    Source,
    Direction,
    Gap,
    Padding,
    Grow,
}

#[derive(Debug, Clone, Default)]
pub struct Props(HashMap<PropType, Prop>);

impl Props {
    pub fn new() -> Self {
        Props::default()
    }
    pub fn insert(&mut self, prop_type: PropType, prop: Prop) {
        self.0.insert(prop_type, prop);
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn get_string(&self, prop: PropType) -> Option<&str> {
        if let Some(Prop::String(value)) = self.0.get(&prop) {
            Some(value)
        } else {
            None
        }
    }
    pub fn get(&self, prop: PropType) -> Option<&Prop> {
        self.0.get(&prop)
    }
    pub fn get_usize(&self, prop: PropType) -> Option<usize> {
        if let Some(Prop::Usize(val)) = self.0.get(&prop) {
            Some(*val)
        } else {
            None
        }
    }
    pub fn get_float(&self, prop: PropType) -> Option<f64> {
        if let Some(Prop::Float(val)) = self.0.get(&prop) {
            Some(*val)
        } else {
            None
        }
    }
    pub fn get_bool(&self, prop: PropType) -> Option<bool> {
        if let Some(Prop::Bool(val)) = self.0.get(&prop) {
            Some(*val)
        } else {
            None
        }
    }
}

/// ui! macro for easily creating ui element trees. The return type is `Box<window_of_opportunity::element::Element>`
///
/// You can reference a custom component (which must implement `Component`)
/// ```
/// use window_of_opportunity::{
/// element::Element,
/// ui,
/// };
///
/// #[derive(Debug)]
/// struct CustomComponent {}
///
/// impl window_of_opportunity::component::Component for CustomComponent {
///     fn render(&self, ctx: &window_of_opportunity::state::Ctx, mut children: Vec<Box<window_of_opportunity::element::Element>>) -> Box<window_of_opportunity::element::Element> {
///         if children.is_empty() {
///             ui! { Button {{ Text { "Click Me" }}}}
///         } else {
///             let child = children.remove(0);
///             ui! {
///                 Div { CHILDREN child }
///             }
///         }
///         // note any children passed to the component are captured with the literal `CHILDREN` - one child at a time
///     }
/// }
///
/// let my_tree = ui! {
///    Window width(400.) {
///         {
///             Div {
///                 { CustomComponent }
///             }
///         }

/// }};
/// ```
/// *Note* `width(400.)` is a prop. Other props shown below.
///
/// All elements apart from Text can have children. Children are a set of `{}` surrounded by an initial set of `{}`
///
/// Elements and their props:
/// * Window
///     * `title(string)`
///     * `on_resize(|&State, w: f64, h: f64)`
/// * Div
///     * `direction(string - column/row)`
/// * Button
///     * `on_click(window_of_opportunity::state::Event)`
/// * Input
///     * `placeholder(string)`
///     * `on_change(|&State, String|)`
/// * Image
/// * List
///     * `rows(int)`
///     * `on_display_item(|Ctx, usize| -> Box<window_of_opportunity::element::Element>)`
/// * Text
///     * `color(string)`
///     * `font_size(int)`
///
/// In addition, there are generic props that can be applied to all elements:
/// * `gap(f64)` for space between multiple siblings (not the beginning or end)
/// * `height(f64)`
/// * `width(f64)`
/// * `padding(f64)`
/// * `background(string)`
///
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
                props: $crate::element::Props::new(),
                handlers: HashMap::new(),
                children: vec![],
            };
            $( ui!(@prop el, $prop ($($val)*)); )*
            Box::new(el)
        }
    };
    (Text $contents:expr) => {
        {
            Box::new($crate::element::Element {
                element_type: $crate::element::ElementType::Text($contents.into()),
                props: $crate::element::Props::new(),
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
                props: $crate::element::Props::new(),
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
                props: $crate::element::Props::new(),
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
                props: $crate::element::Props::new(),
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
                props: $crate::element::Props::new(),
                handlers: HashMap::new(),
                children: vec![],
            };
            $( ui!(@prop el, $prop ($($val)*)); )*
            Box::new(el)
        }
    };
    // ---- prop parsing ----
    // Handler props hold handler closures; every other prop is TYPED — each
    // arm names its PropType and Prop variant, and `@type_prop` inserts the
    // value (`.into()` type-checks at the call site).
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
    (@prop $el:ident, background($($val:tt)*)) => {
        ui!(@type_prop $el Background String ($($val)*));
    };
    (@prop $el:ident, rows($($val:tt)*)) => {
        ui!(@type_prop $el Rows Usize ($($val)*));
    };
    (@prop $el:ident, width($($val:tt)*)) => {
        ui!(@type_prop $el Width Float ($($val)*));
    };
    (@prop $el:ident, height($($val:tt)*)) => {
        ui!(@type_prop $el Height Float ($($val)*));
    };
    (@prop $el:ident, padding($($val:tt)*)) => {
        ui!(@type_prop $el Padding Float ($($val)*));
    };
    (@prop $el:ident, grow($($val:tt)*)) => {
        ui!(@type_prop $el Grow Bool ($($val)*));
    };
    (@prop $el:ident, resizable($($val:tt)*)) => {
        ui!(@type_prop $el Resizable Bool ($($val)*));
    };
    (@prop $el:ident, src($($val:tt)*)) => {
        ui!(@type_prop $el Source String ($($val)*));
    };
    (@prop $el:ident, gap($($val:tt)*)) => {
        ui!(@type_prop $el Gap Float ($($val)*));
    };
    (@prop $el:ident, font_size($($val:tt)*)) => {
        ui!(@type_prop $el FontSize Float ($($val)*));
    };
    (@prop $el:ident, direction($($val:tt)*)) => {
        ui!(@type_prop $el Direction String ($($val)*));
    };
    (@prop $el:ident, value($($val:tt)*)) => {
        ui!(@type_prop $el Value String ($($val)*));
    };
    (@prop $el:ident, placeholder($($val:tt)*)) => {
        ui!(@type_prop $el Placeholder String ($($val)*));
    };
    (@prop $el:ident, title($($val:tt)*)) => {
        ui!(@type_prop $el Title String ($($val)*));
    };
    (@prop $el:ident, color($($val:tt)*)) => {
        ui!(@type_prop $el Color String ($($val)*));
    };
    // An unknown prop fails LOUDLY at the call site — with a message,
    // not with a missing-variant error from inside this macro.
    (@prop $el:ident, $prop:ident($($val:tt)*)) => {
        compile_error!(concat!(
            "unknown prop `", stringify!($prop),
            "` — known props: width, height, padding, gap, grow, direction, background, color, font_size, title, value, placeholder, src, resizable, rows",
            " (handlers: on_click, on_resize, on_change, on_display_item)"
        ));
    };
    (@type_prop $el:ident $prop:ident $prop_type:ident ($($val:tt)*)) => {
        $el.props.insert($crate::element::PropType::$prop, $crate::element::Prop::$prop_type($($val.into())*));
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
            el.props.get_string(PropType::Background),
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
        let el = crate::ui! { Text width(1) height(2) { item.clone() } };
        assert!(matches!(&el.element_type, ElementType::Text(t) if t == "field"));
        assert_eq!(el.props.get_float(PropType::Width), Some(1.));
        assert_eq!(el.props.get_float(PropType::Height), Some(2.));
    }
}
// (appended coverage for the typed-prop arms)
#[cfg(test)]
mod typed_prop_tests {
    #[test]
    fn every_framework_consumed_prop_has_an_arm() {
        // these three props are read by layout/window_spec/Image mount —
        // if a macro arm goes missing, this test fails to compile
        let el = crate::ui! { Image src("slot") width(2.) };
        assert_eq!(
            el.props.get_string(crate::element::PropType::Source),
            Some("slot")
        );

        let el = crate::ui! { Div grow(true) gap(4.) { { crate::ui! { Text "x" } } } };
        assert_eq!(
            el.props.get_bool(crate::element::PropType::Grow),
            Some(true)
        );
        assert_eq!(el.props.get_float(crate::element::PropType::Gap), Some(4.));

        let el = crate::ui! { Window width(400.) resizable(false) };
        assert_eq!(
            el.props.get_bool(crate::element::PropType::Resizable),
            Some(false)
        );
    }
}

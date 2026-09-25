use window_of_opportunity::{app::Application, component::Component, ui};

/// A component with its own state: the count lives in a keyed slot that
/// survives re-renders and full remounts — this is the hooks model.
#[derive(Debug)]
struct Counter {}

impl Component for Counter {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let count = ctx.use_state("count", || 0);
        let increment = ctx.set_state("count", |c: &mut i32| *c += 1);

        ui! {
            Button on_click(increment) { { Text format!("count: {count}") } }
        }
    }
}

#[derive(Debug)]
struct Pane {}

impl Component for Pane {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        mut children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let first = ctx.use_state("first", || true);
        let switch = ctx.set_state(
            "first",
            |f: &mut bool| if *f == true { *f = false } else { *f = true },
        );

        let text = if first {
            "first from pane"
        } else {
            "second from pane"
        };

        if children.is_empty() {
            ui! { Div {
                    { Button on_click(switch)  {{ Text text.to_string()}}}
                    { List rows(2usize) on_display_item(|ctx, row| {
                        // rows are element trees like any other — styled with
                        // the same props. This one reads state (the Pane's
                        // `first` slot), so toggling the button re-rows the list.
                        let first = ctx.use_state("first", || true);
                        let items = if first { ["one", "two"] } else { ["ein", "zwei"] };
                        let color = if row == 0 { "blue" } else { "green" };
                        ui! {
                            Div height(24.) padding(4.) background(color) {
                                { Text format!("row {row}: {}", items[row]) }
                            }
                        }
                    }) }
                  // { List data(vec!["some more things".to_string(), "again".to_string()]) }
                }
            }
        } else {
            let child = children.remove(0);
            ui! { Div { CHILDREN child }}
        }
    }
}

/// A controlled text input: the echo Label and the field both read the same
/// slot; every keystroke runs on_change → state → full re-render (and a
/// full remount — Phase 1: the field is destroyed per keystroke, and only
/// positional focus restoration keeps typing alive).
#[derive(Debug)]
struct Typing {}

impl Component for Typing {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let text = ctx.use_state("text", || String::new());

        ui! {
            Div direction("column") gap(6.) {
                { Text format!("echo: {text}") }
                { Input value(text) placeholder("type here...")
                        on_change(|state, t| state.update("text", |s: &mut String| *s = t)) }
            }
        }
    }
}

#[derive(Debug)]
struct Window {}

impl Component for Window {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let (w, h) = ctx.use_state("window/size", || (640., 820.));
        let first = ctx.use_state("first", || true);
        let title = if first {
            "One message"
        } else {
            "Another message"
        };
        ui! {
            Window width(w) height(h) title(title)
                   on_resize(|state, w, h|
                       state.update("window/size", |s: &mut (f64, f64)| *s = (w, h))) {
                { Div direction("column") gap(10.) padding(16.) {
                    { Pane }
                    { Div height(80.) background("blue") {} }
                    { Div direction("row") gap(10.) height(60.) {
                        { Button { Text "Hello, and this should mean a bigger button" } }
                        { Button { Text "Two" } }
                        { Div width(80.) background("red") {} }
                    } }
                    { Counter }
                    { Typing }
                    { Div height(40.) background("green") {} } // grow(true) here would be 0 tall: nothing to absorb in hug mode
                } }
            }
        }
    }
}

fn main() {
    let window = Box::new(Window {});
    let app = Application {};

    app.run(window, |_state, _message: ()| ())
}

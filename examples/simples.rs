#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::sync::Arc;

use window_of_opportunity::{
    app::Application,
    component::Component,
    element::{BlitFrame, Pixel},
    ui,
};

/// A component with its own state: the count lives in a keyed slot that
/// survives re-renders and full remounts — this is the hooks model.
#[derive(Debug, Default)]
struct Counter {
    thing: String,
}

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

#[derive(Debug, Default)]
struct Pane {}

impl Component for Pane {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        mut children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let first = ctx.use_state("first", || true);
        let switch = ctx.set_state("first", |f: &mut bool| *f = !*f);

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
                            Div padding(4.) background(color) {
                                { Text format!("row {row}: {}", items[row]) }
                            }
                        }
                    }) }
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
#[derive(Debug, Default)]
struct Typing {}

impl Component for Typing {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let text = ctx.use_state("text", String::new);

        ui! {
            Div direction("column") gap(6.) {
                { Text format!("echo: {text}") }
                { Input value(text) placeholder("type here...")
                        on_change(|state, t| state.update("text", |s: &mut String| *s = t)) }
            }
        }
    }
}

#[derive(Debug, Default)]
struct Imaging {}

impl Component for Imaging {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        // --- the RepaintHandle in action -------------------------------
        // A ticker thread says "hey, re-render" five times a second, and
        // this render simply SAMPLES THE CLOCK for the gradient — the
        // content is derived at render time, the handle is only the
        // invalidation signal. That's the whole idea: whatever changed
        // (time, a sensor, a stream) lives outside the framework; the
        // handle just tells it to run again. `repaint(repaint)` on the
        // Image binds the same handle to that element, so handle.repaint()
        // redraws JUST it (no render) and handle.rerender() runs the loop.
        let repaint = ctx.use_repaint("anim");
        ctx.use_state("ticker", || {
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    repaint.rerender();
                }
            });
        });
        let phase = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_millis() as f64 / 25.) as i128)
            .unwrap_or(0) as usize;
        let img = gradient_frame(phase);

        ui! { Image source(frame(img)) repaint(repaint) grow(true) {} }
    }
}

#[derive(Debug, Default)]
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
                { Div direction("column") gap(10.) grow(true) padding(10., 20., 30., 40.)  { // note, we have t, r, b, l padding
                    { Pane }
                    { Div height(80.) background("black") {} }
                    { Div direction("row") gap(10.) height(60.) {
                        { Button { Text "Hello, and this should mean a bigger button" } }
                        { Button { Text "Two" } }
                        { Div width(80.) background("red") {} }
                    } }
                    { Counter thing("blah".into())} // a prop into counter
                    { Typing }
                    { Imaging }
                } }
            }
        }
    }
}

/// A gradient that shifts with `phase` — derived at render time like any
/// good React output. `version` ticks with the phase so the image widget
/// can tell frames apart (and skip re-blitting unchanged ones).
fn gradient_frame(phase: usize) -> BlitFrame {
    let (width, height) = (600, 400);
    let mut pixels = vec![Pixel(0); width * height];
    for y in 0..height {
        for x in 0..width {
            let b = ((x + phase * 17) & 0xff) as u32;
            let g = (y & 0xff) as u32;
            pixels[y * width + x] = Pixel(0xFF00_0000 | (g << 8) | b);
        }
    }
    let mut frame = BlitFrame::new(width, height);
    frame.pixels = Arc::new(pixels);
    frame.version = phase as u64;
    frame
}

fn main() {
    let window = Box::new(Window {});
    let app = Application {};

    app.run(window, |_state, _message: ()| ())
}

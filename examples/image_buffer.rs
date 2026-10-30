#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::sync::Arc;

use window_of_opportunity::{
    app::Application,
    component::Component,
    element::{BlitFrame, Pixel},
    ui,
};

#[derive(Debug, Default)]
struct Imaging {}

impl Component for Imaging {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        // --- the RepaintHandle in action -------------------------------
        // A ticker thread says "hey, re-render" thirty times a second, and
        // this render simply SAMPLES THE CLOCK for the gradient — the
        // content is derived at render time, the handle is only the
        // invalidation signal. That's the whole idea: whatever changed
        // (time, a sensor, a stream) lives outside the framework; the
        // handle just tells it to run again. `repaint(repaint)` on the
        // Image binds the same handle to that element, so handle.repaint()
        // redraws JUST it (no render) and handle.rerender() runs the loop.
        let repaint = ctx.use_repaint("anim");
        let (w, h) = ctx.use_state("image/size", || (640., 820.));
        ctx.use_state("ticker", || {
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(1000 / 30));
                    repaint.rerender();
                }
            });
        });
        let start = ctx.use_state("start", std::time::Instant::now);
        let offset = start.elapsed().as_millis() / 5;
        // The frame follows the box — but at QUARTER resolution, stretched
        // back to full size by the blit. Per-render pixel generation is
        // O(area): generating box-sized frames every tick saturates the UI
        // thread as the window grows (maximize on a 5K display made the
        // app stop responding to input in debug builds). Fit modes + a
        // cached scaled bitmap are the real answer; quarter-res keeps the
        // box-following demo honest and affordable.
        let img = gradient_frame(offset as usize, (w) as usize, (h) as usize);

        ui! { Image on_resize(|state, w, h|
        state.update("image/size", |d|  *d = (w, h))) source(frame(img)) repaint(repaint) grow(true) {} }
    }
}

#[derive(Debug, Default)]
struct Window {}

impl Component for Window {
    fn render(
        &self,
        _: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        ui! {
            Window width(800) height(600) title("Image Buffer Animation") {
                    { Imaging }
            }
        }
    }
}

/// A gradient that shifts with `offset` — derived at render time like any
/// good React output. `version` ticks with the offset so the image widget
/// can tell frames apart (and skip re-blitting unchanged ones).
fn gradient_frame(offset: usize, width: usize, height: usize) -> BlitFrame {
    // updating at 30 frames a second is tricky on my m5 at 5k res, so restrict it
    let max_w = 1024;
    let max_y = 768;
    let width = if width > max_w { max_w } else { width };
    let height = if height > max_y { max_y } else { height };
    let mut pixels = vec![Pixel(0); width * height];
    for y in 0..height {
        for x in 0..width {
            let b = ((x + offset) & 0xff) as u32;
            let g = (y & 0xff) as u32;
            pixels[y * width + x] = Pixel(0xFF00_0000 | (g << 8) | b);
        }
    }
    let mut frame = BlitFrame::new(width, height);
    frame.pixels = Arc::new(pixels);
    frame.version = offset as u64;
    frame
}

fn main() {
    let window = Box::new(Window {});
    let app = Application {};

    app.run(window, |_state, _message: ()| ())
}

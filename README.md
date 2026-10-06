# Window_of_opportunity

A WIP, aiming to implement a react style library for creating native (as in, using the actual OS primitives) GUIs. 

Currently targetting Win32 directly via the windows crate, and Macos with AppKit (using Cacao under the hood).

## Features

* Input boxes support internationalization because they are the native input boxes.
* Small binaries - the todos example compiles to 221kb for Windows, and 1.3Mb for Mac, in release.

## Getting started

The simplest possible app would look like this:

```rust
use window_of_opportunity::{app::Application, component::Component, ui};

#[derive(Debug, Default)]
struct Window {}
impl Component for Window {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let first = ctx.use_state("first", || true);
        let switch = ctx.set_state("first", |f: &mut bool| *f = !*f);
        let title = if first {
            "One message"
        } else {
            "Another message"
        };
        ui! {
            Window width(600.) height(500) title(title) {
                { Div direction("column") gap(40.) padding(16.) {
                    { Div height(80.) background("blue") {} }
                    { Div direction("row") gap(10.) height(60.) {
                        { Button { Text "Hello, and this should mean a bigger button" } }
                        { Button on_click(switch) { Text "Click to change the title" } }
                        { Div grow(true)  background("red") {} }
                    } }
                    { Div height(40.) background("green") {} }
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
```

Checkout the examples folder for the kinds of controls you can currently use, and how you'd use the different handlers. Run using `cargo run --example todos` or any of the other examples.

## Project Status
The very basic controls are there, and there are hooks for some state.

| Feature | Macos | Win32 | Gtk |
| ------- | ----- | ----- | --- |
| Button | ✅ | ✅ | ❌ |
| Label | ✅ | ✅ | ❌ |
| Image (using BlitFrame) |  ✅ | ❌ | ❌ |
| Input |  ✅ | ✅ | ❌ |
| Window |  ✅ | ✅ | ❌ |
| Window resizing |  ✅ | ✅ | ❌ |
| Window title |  ✅ | ✅ | ❌ |
| Layout |  ✅ | ✅ | ❌ |
| List | ✅ | ✅ (using ListView with customdraw for items) | ❌ |
| Test target | ❌ | ❌ | ❌ |

* Move to directly use objc2 for Macos
* Handle vec based lists
* Add more elements - scrollviews, radiobuttons, comboboxes, selectboxes.
* Add more properties - border, rounded corners, other events
* Get working with win32 (almost feature parity with macos)
* Get working with gtk.
* Add a test target for writing automated tests against the virtual dom
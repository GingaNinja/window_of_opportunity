# Window_of_opportunity

A massively under-developed WIP, aiming to implement a react style library for creating native (as in, using the actual OS primitives) GUIs. 

Currently targetting Macos with AppKit (using Cacao under the hood), but wanting to add some win32 waiting in the sidelines.

## Features

* Input boxes support internationalization because they are the native input boxes.
* Small binaries - currently the library is less than 2k lines, plus a dependency on Cacao.

## Getting started

The simplest possible app would look like this:

```rust
use window_of_opportunity::{app::Application, component::Component, ui};

#[derive(Debug)]
struct Window {}
impl Component for Window {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        ui! {
            Window width(600.) height(500) title("Hello World!") {
                { Div direction("column") gap(40.) padding(16.) {
                    { Div height(80.) background("blue") {} }
                    { Div direction("row") gap(10.) height(60.) {
                        { Button { Text "Hello, and this should mean a bigger button" } }
                        { Button { Text "Two" } }
                        { Div width(80.) background("red") {} }
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

Checkout the examples folder, there's at least one example in there which is doing not very much, but shows click event handlers.

## Project Status
Note, hardly anything works, but creating a basic application is possible, and there are hooks for some state.

| Feature | Macos | Win32 | Gtk |
| ------- | ----- | ----- | --- |
| Button | ✅ | ❌ | ❌ |
| Label | ✅ | ❌ | ❌ |
| Image (using BlitFrame) |  ✅ | ❌ | ❌ |
| Input |  ✅ | ❌ | ❌ |
| Window |  ✅ | ❌ | ❌ |
| Window resizing |  ✅ | ❌ | ❌ |
| Window title |  ✅ | ❌ | ❌ |
| Layout |  ✅ | ❌ | ❌ |
| List | ✅ | ❌ | ❌ |
| Test target | ❌ | ❌ | ❌ |

* Handle vec based lists
* Add more elements - scrollviews, radiobuttons, comboboxes, selectboxes.
* Add more properties - border, rounded corners, other events
* Get working with win32
* Get working with gtk.
* Add a test target for writing automated tests against the virtual dom
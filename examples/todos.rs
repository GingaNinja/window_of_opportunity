use window_of_opportunity::{app::Application, component::Component, ui};

struct Todo {
    title: String,
    status: TodoStatus,
}

enum TodoStatus {
    Complete,
    Incomplete,
}

#[derive(Debug)]
struct TodoView {}

impl Component for TodoView {
    fn render(
        &self,
        _ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        ui! {
            List rows(2) on_display_item(|_ctx, row| {
                let items = vec![
                    Todo { title: "Stuff".to_string(), status: TodoStatus::Incomplete },
                    Todo { title: "Done and dusted".to_string(), status: TodoStatus::Complete },
                ];
                let item = &items[row];
                ui! {
                    Div direction("column") {
                        { Text item.title.clone() }
                        // a `match` as a child — every arm returns an element
                        { match item.status {
                            TodoStatus::Complete => ui! { Text color("blue") font_size(10) { "Complete".to_string() } },
                            TodoStatus::Incomplete => ui! { Text color("red") font_size(10) { "Incomplete".to_string() } },
                        } }
                    }
                }
            })
        }
    }
}

#[derive(Debug)]
struct Window {}

impl Component for Window {
    fn render(
        &self,
        _ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        ui! {
            Window width(400) height(400)  {
                { Div direction("column") background("blue") padding(20.) {
                    { TodoView }
                } }
            }
        }
    }
}

fn main() {
    let window = Box::new(Window {});
    let app = Application {};

    app.run(window, |_state, _message: ()| ());
}

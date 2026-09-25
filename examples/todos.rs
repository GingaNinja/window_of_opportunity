use std::{cell::RefCell, rc::Rc};

use window_of_opportunity::{app::Application, component::Component, ui};

#[derive(Clone)]
struct Todo {
    title: String,
    status: TodoStatus,
}

#[derive(Clone)]
enum TodoStatus {
    Complete,
    Incomplete,
}

#[derive(Debug, Default)]
struct TodoView {}

/// sets up a shared seed so that whichever `use_state` goes first gets the same seed
fn seed() -> Rc<RefCell<Vec<Todo>>> {
    // Rc RefCell means we are not copying the list, and we can add/remove items
    Rc::new(RefCell::new(vec![
        Todo {
            title: "Stuff".into(),
            status: TodoStatus::Incomplete,
        },
        Todo {
            title: "Done and dusted".to_string(),
            status: TodoStatus::Complete,
        },
    ]))
}

impl Component for TodoView {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        let items = ctx.use_state("todos", seed);

        let add_todo = ctx.set_state("todos", |t: &mut Rc<RefCell<Vec<Todo>>>| {
            // for now, just a silly hard-coded `Todo`, but creating a new todo should be relatively trivial
            let mut t = t.borrow_mut();
            t.push(Todo {
                title: "came from click".into(),
                status: TodoStatus::Incomplete,
            })
        });

        let item_count = { items.borrow().len() };

        ui! {
            Div {
                { Button on_click(add_todo) { Text "Add row" }}
                { List rows(item_count) on_display_item(|ctx, row| {
                    let items = ctx.use_state("todos", seed);
                    let item = {
                        let rows = &items.borrow();
                        rows[row].clone()
                    };
                    ui! {
                        Div direction("column") {
                            { Text item.title.clone() }
                            // a `match` as a child — every arm returns an element
                            { match item.status {
                                TodoStatus::Complete => ui! { Text color("blue") font_size(10.) { "Complete".to_string() } },
                                TodoStatus::Incomplete => ui! { Text color("red") font_size(10.) { "Incomplete".to_string() } },
                            } }
                        }
                    }
                })}
            }

        }
    }
}

#[derive(Debug, Default)]
struct Window {}

impl Component for Window {
    fn render(
        &self,
        _ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        ui! {
            Window width(400.) height(400.)  {
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

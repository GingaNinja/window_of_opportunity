use window_of_opportunity::{app::Application, component::Component, ui};

#[derive(Debug, Default)]
struct Window {}
impl Component for Window {
    fn render(
        &self,
        _ctx: &window_of_opportunity::state::Ctx,
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

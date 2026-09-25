use std::fmt::Debug;

use crate::{element::Element, state::Ctx};

/// A component. Constructed by `ui!` via `Default` + field assignment
/// (`TodoView title("hi")` sets `title` after `TodoView::default()`), so
/// components should implement `Default` — on the STRUCT, not as a trait
/// supertrait (`Default` isn't object-safe, and the tree stores
/// `Rc<dyn Component>`). Hand-write `Default` when fields have non-`Default`
/// types. Fields settable via `ui!` props must be visible at the call site.
pub trait Component: Debug {
    fn render(&self, ctx: &Ctx, children: Vec<Box<Element>>) -> Box<Element>;
}

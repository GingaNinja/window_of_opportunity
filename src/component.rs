use std::fmt::Debug;

use crate::{element::Element, state::Ctx};

pub trait Component: Debug {
    fn render(&self, ctx: &Ctx, children: Vec<Box<Element>>) -> Box<Element>;
}

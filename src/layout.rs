// ---------------------------------------------------------------------------
// Flex vocabulary
// ---------------------------------------------------------------------------
use crate::element::{PropType, Props};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Row,
    Column,
}

/// A typed view of an element's props, in flexbox vocabulary. This is deliberately
/// close to Taffy's vocabulary — when this constraint-based engine runs out of
/// expressiveness (wrap, weighted grow), `FlexStyle` becomes `FlexStyle::to_taffy()`
/// and the components never notice.
#[derive(Debug, Clone, Default)]
pub struct FlexStyle {
    pub direction: Direction,
    pub gap: f64,
    pub padding: f64,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub grow: bool,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Column
    }
}

impl FlexStyle {
    pub fn from_props(props: &Props) -> Self {
        FlexStyle {
            direction: match props.get_string(PropType::Direction) {
                Some("row") => Direction::Row,
                _ => Direction::Column,
            },
            gap: props.get_float(PropType::Gap).unwrap_or_default(),
            padding: props.get_float(PropType::Padding).unwrap_or_default(),
            width: props.get_float(PropType::Width),
            height: props.get_float(PropType::Height),
            grow: props.get_bool(PropType::Grow).unwrap_or_default(),
        }
    }
}

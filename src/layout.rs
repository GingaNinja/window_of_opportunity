// ---------------------------------------------------------------------------
// Flex vocabulary
// ---------------------------------------------------------------------------

use std::collections::HashMap;

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
    pub fn from_props(props: &HashMap<String, String>) -> Self {
        let num = |key: &str| props.get(key).and_then(|value| value.parse().ok());

        FlexStyle {
            direction: match props.get("direction").map(String::as_str) {
                Some("row") => Direction::Row,
                _ => Direction::Column,
            },
            gap: num("gap").unwrap_or(0.),
            padding: num("padding").unwrap_or(0.),
            width: num("width"),
            height: num("height"),
            grow: props.get("grow").map(|v| v == "true").unwrap_or(false),
        }
    }
}

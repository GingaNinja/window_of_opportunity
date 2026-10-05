// ---------------------------------------------------------------------------
// The stack layout engine — the hand-rolled AutoLayout replacement the port
// notes call for (swap for taffy when the vocabulary outgrows it).
//
// A port of `container_constraints` semantics to rect math:
//   * column (default) / row stacking, `gap` between siblings
//   * `padding` insets the content box
//   * fixed width/height props win over natural sizes
//   * cross axis: children stretch to the container's inner size
//   * `grow`: the last grow child absorbs all remaining main-axis slack so
//     the chain ends flush (the required end-pin of the constraint version)
//   * hug: without a fixed size a box sizes to its content
//
// Simplification vs the cacao side: arrangement runs wholesale after each
// render instead of diffing constraints — SetWindowPos on unchanged rects
// is cheap and (unlike AutoLayout) never disturbs focus.
// ---------------------------------------------------------------------------

use windows::Win32::{
    Foundation::{HWND, SIZE},
    Graphics::Gdi::{GetDC, GetTextExtentPoint32W, ReleaseDC},
    UI::WindowsAndMessaging::{GetParent, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos},
};

use crate::{
    element::{Element, ElementType, button_label},
    layout::{Direction, FlexStyle},
};

use super::{util::get_utf16_vec, widgets::Widget};

/// A layout box (client-area coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Arranges the tree inside `area`, positioning every leaf control.
pub fn arrange(el: &Element, widget: &mut Widget, area: Rect) {
    let style = FlexStyle::from_props(&el.props);
    let pad = style.padding as i32;

    // a fixed size on the box itself wins over the area it was handed, unless it's the window (if it can be resized)
    let (x, y) = (area.x, area.y);
    let (w, h) = if let ElementType::Window = el.element_type {
        (area.w, area.h)
    } else {
        (
            style.width.map(|w| w as i32).unwrap_or(area.w),
            style.height.map(|h| h as i32).unwrap_or(area.h),
        )
    };

    let Widget::Container { hwnd, children, .. } = widget else {
        place(widget, x, y, w, h);
        return;
    };

    // the container's own window goes here (the root excepted — see
    // place_hwnd)
    place_hwnd(*hwnd, x, y, w, h);

    // children live in the container's CLIENT space, so the inner box is
    // 0-based no matter where the container sits in its own parent
    let inner = Rect {
        x: pad,
        y: pad,
        w: w - 2 * pad,
        h: h - 2 * pad,
    };
    if el.children.is_empty() || inner.w <= 0 || inner.h <= 0 {
        return;
    }

    let main_is_row = matches!(style.direction, Direction::Row);
    let gap = style.gap as i32;

    // natural main-axis sizes first, so grow knows the slack
    let naturals: Vec<(i32, i32)> = el
        .children
        .iter()
        .zip(children.iter())
        .map(|(child_el, child_widget)| natural(child_el, child_widget))
        .collect();

    let total_main: i32 = naturals
        .iter()
        .map(|(w, h)| if main_is_row { *w } else { *h })
        .sum::<i32>()
        + gap * (el.children.len().saturating_sub(1) as i32);
    let inner_main = if main_is_row { inner.w } else { inner.h };
    let slack = (inner_main - total_main).max(0); // containment: overflow clips

    // the LAST grow child absorbs all slack (matching the constraint
    // version's required end-pin, which stretches exactly one box)
    let last_grow = el
        .children
        .iter()
        .rposition(|child| FlexStyle::from_props(&child.props).grow);

    let mut cursor = if main_is_row { inner.x } else { inner.y };
    for (index, (child_el, child_widget)) in el.children.iter().zip(children.iter_mut()).enumerate()
    {
        let c_style = FlexStyle::from_props(&child_el.props);
        let (nw, nh) = naturals[index];
        let mut main = if main_is_row { nw } else { nh };
        if Some(index) == last_grow {
            main += slack;
        }

        let (bx, by, bw, bh) = if main_is_row {
            let bw = c_style.width.map(|w| w as i32).unwrap_or(main);
            let bh = c_style.height.map(|h| h as i32).unwrap_or(inner.h);
            (cursor, inner.y, bw, bh)
        } else {
            let bw = c_style.width.map(|w| w as i32).unwrap_or(inner.w);
            let bh = c_style.height.map(|h| h as i32).unwrap_or(main);
            (inner.x, cursor, bw, bh)
        };

        arrange(
            child_el,
            child_widget,
            Rect {
                x: bx,
                y: by,
                w: bw,
                h: bh,
            },
        );
        cursor += (if main_is_row { bw } else { bh }) + gap;
    }
}

/// The natural (hugging) size of a box: leaves measure their control,
/// containers sum their children along the direction. Fixed width/height
/// props always win.
pub fn natural(el: &Element, widget: &Widget) -> (i32, i32) {
    let style = FlexStyle::from_props(&el.props);
    let pad = style.padding as i32;

    let (mut w, mut h) = match (widget, &el.element_type) {
        (Widget::Button { hwnd, .. }, _) => measure_button(*hwnd, &button_label(el)),
        (Widget::Label { hwnd }, ElementType::Text(text)) => measure_text(*hwnd, text),
        (Widget::Container { children, .. }, _) => {
            let gap = style.gap as i32;
            let (mut main, mut cross) = (0, 0);
            for (child_el, child_widget) in el.children.iter().zip(children.iter()) {
                let (cw, ch) = natural(child_el, child_widget);
                match style.direction {
                    Direction::Row => {
                        main += cw;
                        cross = cross.max(ch);
                    }
                    Direction::Column => {
                        main += ch;
                        cross = cross.max(cw);
                    }
                }
            }
            main += gap * (el.children.len().saturating_sub(1) as i32);
            match style.direction {
                Direction::Row => (main + 2 * pad, cross + 2 * pad),
                Direction::Column => (cross + 2 * pad, main + 2 * pad),
            }
        }
        (Widget::List { rows, .. }, _) => {
            // Until the row-height/painting work lands, rows use the default
            // report-row height (one system-font line) — the size the
            // ListView actually paints at, so the item count is visible.
            // Replace with measured rows when painting lands (see the TODO
            // in patch's List arm).
            const ROW_HEIGHT: i32 = 18;
            (80, rows.len() as i32 * ROW_HEIGHT + 2)
        }
        _ => (80, 24),
    };

    if let Some(fw) = style.width {
        w = fw as i32;
    }
    if let Some(fh) = style.height {
        h = fh as i32;
    }
    (w.max(1), h.max(1))
}

fn place(widget: &Widget, x: i32, y: i32, w: i32, h: i32) {
    let hwnd = match widget {
        Widget::Button { hwnd, .. }
        | Widget::Label { hwnd }
        | Widget::Input { hwnd, .. }
        | Widget::List { hwnd, .. } => *hwnd,
        Widget::Container { .. } => return,
    };
    place_hwnd(hwnd, x, y, w, h);
}

fn place_hwnd(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        // the root's hwnd is the top-level window — its position is the
        // user's, never the layout's
        if GetParent(hwnd).is_err() {
            return;
        }
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

fn measure_text(hwnd: HWND, text: &str) -> (i32, i32) {
    let (w, h) = text_extent(hwnd, text);
    (w + 2, h + 4)
}

fn measure_button(hwnd: HWND, text: &str) -> (i32, i32) {
    // text extent + push-button chrome (the step-2 approximation of
    // BCM_GETIDEALSIZE, which arrives with the common-controls work)
    let (w, h) = text_extent(hwnd, text);
    (w + 32, h + 14)
}

fn text_extent(hwnd: HWND, text: &str) -> (i32, i32) {
    let wide = get_utf16_vec(text);
    unsafe {
        let hdc = GetDC(Some(hwnd));
        let mut size = SIZE::default();
        let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
        let _ = ReleaseDC(Some(hwnd), hdc);
        (size.cx, size.cy)
    }
}

// ---------------------------------------------------------------------------
// Row painting: turns a snapshot element tree into pixels inside a row rect.
// The seam for the painting work. Supported subset for now: Text + Div
// backgrounds; Button/Input/Image in painted rows are stubs (live controls
// don't exist in painted rows — the documented capability gap).
//
// Geometry MIRRORS stack::arrange (direction, gap, padding, fixed sizes,
// cross-axis stretch, grow = last grow child absorbs the slack) but
// measures from the DC instead of widgets — rows have no widgets. When the
// painting semantics settle, factor the shared box computation out of
// arrange so the two walks can't drift.
// ---------------------------------------------------------------------------

use windows::Win32::Foundation::{RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetDeviceCaps,
    GetObjectW, GetStockObject, GetTextExtentPoint32W, SelectObject, SetBkMode, SetTextColor,
    DEFAULT_GUI_FONT, DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, HBRUSH, HFONT, HGDIOBJ, HDC,
    LOGFONTW, LOGPIXELSY, TRANSPARENT,
};

use crate::{
    element::{Element, ElementType, PropType},
    layout::{Direction, FlexStyle},
};

use super::{
    app::color_ref,
    stack::Rect,
    util::get_utf16_vec,
};

/// Paints an element tree into `area`. The walk mirrors stack::arrange:
/// containers stack their children (column/row, gap, padding), leaves draw.
pub fn paint_tree(hdc: HDC, el: &Element, area: Rect) {
    let style = FlexStyle::from_props(&el.props);
    let pad = style.padding as i32;

    // the element's own backdrop: skip when absent — whatever is already on
    // the row (native selection, the parent's paint) shows through, like an
    // undecorated Div
    if let Some(bg) = el.props.get_string(PropType::Background) {
        fill(hdc, area, bg);
    }

    match &el.element_type {
        ElementType::Text(text) => unsafe {
            SetBkMode(hdc, TRANSPARENT);
            if let Some(fg) = el.props.get_string(PropType::Color) {
                SetTextColor(hdc, color_ref(fg));
            }
            let _font = FontGuard::apply(hdc, el); // font_size prop, in and out
            let mut wide = get_utf16_vec(text);
            let mut rect = to_rect(area);
            DrawTextW(
                hdc,
                &mut wide,
                &mut rect,
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );
        }

        ElementType::Button | ElementType::Input | ElementType::Image | ElementType::List => {
            // TODO(painting): painted rows can't host live controls —
            // draw a placeholder or nothing (the capability gap lives
            // behind this arm).
        }

        ElementType::Component(_) => {
            unreachable!("rows are expanded at snapshot time (see reconcile::snapshot_rows)")
        }

        // Window/Div: stack the children — the arrange semantics
        _ => {
            let inner = Rect {
                x: area.x + pad,
                y: area.y + pad,
                w: area.w - 2 * pad,
                h: area.h - 2 * pad,
            };
            if el.children.is_empty() || inner.w <= 0 || inner.h <= 0 {
                return;
            }

            let main_is_row = matches!(style.direction, Direction::Row);
            let gap = style.gap as i32;

            let naturals: Vec<(i32, i32)> =
                el.children.iter().map(|child| natural_size(hdc, child)).collect();
            let total_main: i32 = naturals
                .iter()
                .map(|(w, h)| if main_is_row { *w } else { *h })
                .sum::<i32>()
                + gap * (el.children.len().saturating_sub(1) as i32);
            let inner_main = if main_is_row { inner.w } else { inner.h };
            let slack = (inner_main - total_main).max(0);

            // the LAST grow child absorbs all slack (the required end-pin
            // of the constraint version, stack::arrange's rule)
            let last_grow = el
                .children
                .iter()
                .rposition(|child| FlexStyle::from_props(&child.props).grow);

            let mut cursor = if main_is_row { inner.x } else { inner.y };
            for (index, child) in el.children.iter().enumerate() {
                let c_style = FlexStyle::from_props(&child.props);
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

                paint_tree(
                    hdc,
                    child,
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
    }
}

/// The natural (hugging) size of an element, measured from the DC — rows
/// have no widgets. Also the seam the row-height work wants (the tallest
/// row = max over `natural_size` for each snapshot row).
pub fn natural_size(hdc: HDC, el: &Element) -> (i32, i32) {
    let style = FlexStyle::from_props(&el.props);
    let pad = style.padding as i32;

    let (mut w, mut h) = match &el.element_type {
        ElementType::Text(text) => {
            // measure under the same font painting will use — the row
            // heights derived from this depend on it
            let _font = FontGuard::apply(hdc, el);
            let (tw, th) = text_extent(hdc, text);
            (tw + 8, th + 4) // label breathing room
        }
        _ if !el.children.is_empty() => {
            let gap = style.gap as i32;
            let (mut main, mut cross) = (0, 0);
            for child in el.children.iter() {
                let (cw, ch) = natural_size(hdc, child);
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
        // TODO(painting): button/input/image placeholders
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

/// Runs with the element's font_size prop applied to the DC — an HFONT at
/// the requested size (POINTS, like macOS's `Font::system`), cloned from
/// the system UI font's face/weight so it looks native at any size. The
/// DC is the control's, shared across rows: drop restores whatever was
/// selected and frees the font. Without the prop nothing changes — at
/// custom-draw time the control's own font is already selected.
struct FontGuard {
    hdc: HDC,
    font: Option<HFONT>,
    previous: Option<HGDIOBJ>,
}

impl FontGuard {
    fn apply(hdc: HDC, el: &Element) -> Self {
        let Some(size) = el.props.get_float(PropType::FontSize) else {
            return Self {
                hdc,
                font: None,
                previous: None,
            };
        };
        unsafe {
            // points → pixels (96 DPI ⇒ 1pt = 1.33px)
            let pixels = ((size * GetDeviceCaps(Some(hdc), LOGPIXELSY) as f64 / 72.0).round()) as i32;
            // the stock GUI font's face and weight, at the requested size
            let mut lf = LOGFONTW::default();
            GetObjectW(
                GetStockObject(DEFAULT_GUI_FONT),
                std::mem::size_of::<LOGFONTW>() as i32,
                Some(&mut lf as *mut LOGFONTW as *mut _),
            );
            lf.lfHeight = -pixels; // negative = character height in pixels
            let font = CreateFontIndirectW(&lf);
            let previous = SelectObject(hdc, font.into());
            Self {
                hdc,
                font: Some(font),
                previous: Some(previous),
            }
        }
    }
}

impl Drop for FontGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(previous) = self.previous {
                SelectObject(self.hdc, previous);
            }
            if let Some(font) = self.font {
                let _ = DeleteObject(font.into());
            }
        }
    }
}

/// Fills an area with the named color's brush — the brush is created and
/// freed here (unlike the div windows' owned brushes).
fn fill(hdc: HDC, area: Rect, name: &str) {
    let brush: HBRUSH = unsafe { CreateSolidBrush(color_ref(name)) };
    let rect = to_rect(area);
    unsafe {
        FillRect(hdc, &rect, brush);
        let _ = DeleteObject(brush.into());
    }
}

fn to_rect(area: Rect) -> RECT {
    RECT {
        left: area.x,
        top: area.y,
        right: area.x + area.w,
        bottom: area.y + area.h,
    }
}

fn text_extent(hdc: HDC, text: &str) -> (i32, i32) {
    let wide = get_utf16_vec(text);
    let mut size = SIZE::default();
    unsafe {
        let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
    }
    (size.cx, size.cy)
}

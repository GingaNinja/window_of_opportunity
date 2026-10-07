// ---------------------------------------------------------------------------
// Text rendering state: the ONE place that turns a `font_size` prop (POINTS,
// like macOS's `Font::system`) into an HFONT, and scopes a DC's text state
// (font + color + transparent backdrop) around one draw or measure. Two
// consumers share it so what's measured is always what's drawn:
//
//   * TextGuard — paints list rows and measures text extents under the
//     element's own font/color (paint.rs, dc.rs's text_extent/metrics)
//   * app's apply_font — WM_SETFONT on live controls Windows paints
//
// Points → pixels needs a DC's DPI (GetDeviceCaps LOGPIXELSY), so the
// conversion takes the hdc of the surface being drawn/measured on.
// ---------------------------------------------------------------------------

use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::{
    BACKGROUND_MODE, CreateFontIndirectW, DEFAULT_GUI_FONT, DeleteObject, GetDeviceCaps,
    GetObjectW, GetStockObject, HDC, HFONT, HGDIOBJ, LOGFONTW, LOGPIXELSY, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};

/// font_size in points → the font's pixel height at the DC's DPI (96 DPI
/// ⇒ 1pt = 1.33px).
pub(super) fn scaled_pixels(points: f64, hdc: HDC) -> i32 {
    unsafe { ((points * GetDeviceCaps(Some(hdc), LOGPIXELSY) as f64 / 72.0).round()) as i32 }
}

/// The stock GUI font's face and weight, at the requested pixel height —
/// `lfHeight` is negative = character height in pixels. The CALLER owns
/// the handle: TextGuard frees it after painting, AppState's font cache
/// keeps them alive for WM_SETFONT controls.
pub(super) fn create_font(pixels: i32) -> HFONT {
    unsafe {
        let mut lf = LOGFONTW::default();
        GetObjectW(
            GetStockObject(DEFAULT_GUI_FONT),
            std::mem::size_of::<LOGFONTW>() as i32,
            Some(&mut lf as *mut LOGFONTW as *mut _),
        );
        lf.lfHeight = -pixels;
        CreateFontIndirectW(&lf)
    }
}

/// The text state for one draw/measure pass over a DC: the font at
/// font_size (POINTS, cloned from the system UI font's face/weight so it
/// looks native at any size), the text color, and a TRANSPARENT backdrop
/// (text must never paint its own background box — measuring is blind to
/// it). Drop puts ALL of it back the way it found it and frees the font,
/// so a DC shared across rows/controls inherits nothing. `None`
/// font_size/color change nothing — the DC's current ones apply (at
/// custom-draw time the control's own font is already selected).
pub(super) struct TextGuard {
    hdc: HDC,
    font: Option<HFONT>,
    prev_font: Option<HGDIOBJ>,
    prev_color: Option<COLORREF>,
    prev_bk: i32, // SetBkMode returns the previous mode as a raw int
}

impl TextGuard {
    pub(super) fn apply(hdc: HDC, font_size: Option<f64>, color: Option<COLORREF>) -> Self {
        let prev_color = color.map(|color| unsafe { SetTextColor(hdc, color) });
        let prev_bk = unsafe { SetBkMode(hdc, TRANSPARENT) };
        let (font, prev_font) = match font_size {
            Some(points) => {
                let font = create_font(scaled_pixels(points, hdc));
                (Some(font), Some(unsafe { SelectObject(hdc, font.into()) }))
            }
            None => (None, None),
        };
        Self {
            hdc,
            font,
            prev_font,
            prev_color,
            prev_bk,
        }
    }
}

impl Drop for TextGuard {
    fn drop(&mut self) {
        unsafe {
            // deselect before freeing — DeleteObject refuses a selected font
            if let Some(prev) = self.prev_font {
                SelectObject(self.hdc, prev);
            }
            if let Some(font) = self.font {
                let _ = DeleteObject(font.into());
            }
            if let Some(prev) = self.prev_color {
                SetTextColor(self.hdc, prev);
            }
            SetBkMode(self.hdc, BACKGROUND_MODE(self.prev_bk as u32));
        }
    }
}

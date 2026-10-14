// ---------------------------------------------------------------------------
// Text rendering state: the ONE place that turns a resolved font size
// (device PIXELS — fixup_dpi converts pt/em/px before anything gets here)
// into an HFONT from the system UI font template, and scopes a DC's text
// state (font + color + transparent backdrop) around one draw or measure.
// Two consumers share it so what's measured is always what's drawn:
//
//   * TextGuard — paints list rows and measures text extents under the
//     element's own font/color (paint.rs, dc.rs's text_extent/metrics)
//   * app's apply_font — WM_SETFONT on live controls Windows paints
//
// Pixel heights are POSITIVE everywhere in the app; GDI's lfHeight
// negative-character-height convention flips the sign only in create_font.
// ---------------------------------------------------------------------------

use std::mem;
use std::os::raw::c_void;

use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::{
    BACKGROUND_MODE, CreateFontIndirectW, DeleteObject, HDC, HFONT, HGDIOBJ, LOGFONTW,
    SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::HiDpi::SystemParametersInfoForDpi;
use windows::Win32::UI::WindowsAndMessaging::{NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS};

/// The system UI font (the message font) at `dpi` — the face/weight every
/// control and TextGuard paints with, and the pixel height `1em` means.
/// Its lfHeight is dpi-dependent, so this re-runs on WM_DPICHANGED.
pub(super) fn system_font(dpi: u32) -> LOGFONTW {
    unsafe {
        let mut metrics = NONCLIENTMETRICSW::default();
        metrics.cbSize = mem::size_of::<NONCLIENTMETRICSW>() as u32;
        let _ = SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some(&mut metrics as *mut _ as *mut c_void),
            0,
            dpi,
        );
        metrics.lfMessageFont
    }
}

/// The pixel height a tree's font_size prop carries (fixup_dpi has already
/// resolved pt/em/px to pixels), falling back to the template's own height
/// — the system font at the current dpi — when the prop is somehow absent.
pub(super) fn font_pixels(props: &crate::element::Props, template: LOGFONTW) -> i32 {
    props
        .get_float(crate::element::PropType::FontSize)
        .map_or(-template.lfHeight, |f| f as i32)
}

/// The template's face/weight at the given pixel height. The CALLER owns
/// the handle: TextGuard frees it after painting, AppState's font cache
/// keeps them alive for WM_SETFONT controls.
pub(super) fn create_font(pixel_height: i32, mut font: LOGFONTW) -> HFONT {
    // the app speaks positive pixel heights; GDI wants negative character
    // height — the sign flips exactly here
    font.lfHeight = -pixel_height;
    unsafe { CreateFontIndirectW(&font) }
}

/// The text state for one draw/measure pass over a DC: the font at
/// `font_size` (device pixels, cloned from the system UI font's face/weight
/// so it looks native at any size), the text color, and a TRANSPARENT
/// backdrop (text must never paint its own background box — measuring is
/// blind to it). Drop puts ALL of it back the way it found it and frees
/// the font, so a DC shared across rows/controls inherits nothing. A
/// `None` color changes nothing — the DC's current one applies.
pub(super) struct TextGuard {
    hdc: HDC,
    font: Option<HFONT>,
    prev_font: Option<HGDIOBJ>,
    prev_color: Option<COLORREF>,
    prev_bk: i32, // SetBkMode returns the previous mode as a raw int
}

impl TextGuard {
    pub(super) fn apply(
        hdc: HDC,
        font_size: i32,
        color: Option<COLORREF>,
        log_font: LOGFONTW,
    ) -> Self {
        let prev_color = color.map(|color| unsafe { SetTextColor(hdc, color) });
        let prev_bk = unsafe { SetBkMode(hdc, TRANSPARENT) };
        let (font, prev_font) = {
            let font = create_font(font_size, log_font);
            (Some(font), Some(unsafe { SelectObject(hdc, font.into()) }))
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

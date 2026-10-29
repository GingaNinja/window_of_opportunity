use std::{cell::RefCell, mem, rc::Rc};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{
                D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory,
                ID2D1Factory,
            },
            Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, FillRect, HBRUSH, HDC,
                InvalidateRect, SRCCOPY, SetBkMode, StretchDIBits, TRANSPARENT,
            },
            Imaging::{
                CLSID_WICImagingFactory, IWICImagingFactory, WICDecodeMetadataCacheOnDemand,
            },
        },
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            LibraryLoader::GetModuleHandleW,
        },
        UI::WindowsAndMessaging::*,
    },
    core::*,
};

use crate::{
    element::{ImageSource, Pixel},
    win32::dc,
};

use super::{
    app::{WM_DIV_RESIZED, WM_GET_BG_BRUSH},
    util::load_cursor,
};

const DIV_CLASS: PCWSTR = w!("wo_div");

#[derive(Debug)]
struct Div {
    /// The div's OWN brush copy to paint with (the Widget owns the handle
    /// and frees it on drop) — `None` = inherit the parent's backdrop.
    /// Behind a RefCell because patch updates it through the shared Rc.
    background: RefCell<Option<HBRUSH>>,
    /// The image to paint when this Div is an Image element; `None` = a
    /// plain Div that paints only its background. Patch swaps this through
    /// `set_image_source`, and painting always reads the CURRENT source —
    /// so frames generated after mount show up.
    source: RefCell<Option<ImageSource>>,
    /// display-scale memo: ((frame pixels ptr, box w, box h), scaled copy).
    /// GDI's stretch mangles large sources (probe-verified), so scaling
    /// happens here once per (frame, box) and the blit is 1:1 — exact at
    /// any size. The full fit-mode + quality-scale cache is on the TODO.
    scaled: RefCell<Option<((usize, usize, usize), Vec<Pixel>)>>,
}

/// A Div's child window — the Div state rides along as lpParam (freed in
/// WM_NCDESTROY). `image_source` makes it an Image div; updates go through
/// `set_image_source` / `set_background`.
pub fn create_div(
    parent: HWND,
    background: Option<HBRUSH>,
    image_source: Option<&ImageSource>,
) -> HWND {
    let div_info = Rc::new(Div {
        background: RefCell::new(background),
        source: RefCell::new(image_source.cloned()),
        scaled: RefCell::new(None),
    });
    try_wic();
    let div_info_ptr = Box::into_raw(Box::new(div_info));
    unsafe {
        CreateWindowExW(
            WS_EX_CONTROLPARENT,
            DIV_CLASS,
            w!(""),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | WS_CLIPCHILDREN.0 | WS_CLIPSIBLINGS.0),
            0,
            0,
            10,
            10, // arrange() positions it before anything is visible
            Some(parent),
            None,
            Some(HINSTANCE(GetModuleHandleW(None).unwrap().0)),
            Some(div_info_ptr as *const _ as _),
        )
        .expect("CreateWindowExW div")
    }
}

fn try_wic() {
    unsafe {
        let wic_imaging_factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).unwrap();

        let decoder = wic_imaging_factory
            .CreateDecoderFromFilename(
                w!("ronda.jpg"),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .unwrap();
        let frame = decoder.GetFrame(0).unwrap();
        let mut width = 0;
        let mut height = 0;
        frame.GetSize(&mut width, &mut height).unwrap();
        println!("width: {width}, height: {height}");
        let d2d_options = D2D1_FACTORY_OPTIONS::default();
        let _d2dFactory: ID2D1Factory =
            D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&d2d_options)).unwrap();
    }
}

fn div_state(hwnd: HWND) -> Option<&'static Rc<Div>> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Rc<Div>;
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*ptr })
    }
}

/// Swap an Image div's source (called from patch when the prop changes).
/// Frames are Arc-shared, so this is cheap — and it repaints so the new
/// frame shows.
pub fn set_image_source(hwnd: HWND, source: ImageSource) {
    if let Some(div_info) = div_state(hwnd) {
        *div_info.source.borrow_mut() = Some(source);
        // false: the frame paints over the whole client area anyway
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }
}

/// Swap a div's brush COPY (the Widget owns the real handle and frees it
/// on drop — patch frees the old brush itself).
pub fn set_background(hwnd: HWND, background: Option<HBRUSH>) {
    if let Some(div_info) = div_state(hwnd) {
        *div_info.background.borrow_mut() = background;
        let _ = unsafe { InvalidateRect(Some(hwnd), None, true) };
    }
}

/// The Div class: no class brush (divs paint themselves), the Div state
/// rides in each window's GWLP_USERDATA (set from lpParam at creation).
pub fn register_div_class(hinst: HINSTANCE) {
    unsafe {
        let wc = WNDCLASSEXW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(divproc),
            hInstance: hinst,
            hCursor: load_cursor(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH::default(),
            lpszClassName: DIV_CLASS,
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        // a second Application in one process (the survey tests boot one
        // per test) finds the class already registered — ours, and fine
        assert!(
            atom != 0 || GetLastError() == ERROR_CLASS_ALREADY_EXISTS,
            "RegisterClassExW div class"
        );
    }
}

/// The brush a div paints with: its own if it has one, else the parent's —
/// an undecorated Div shows its parent's backdrop like a transparent
/// NSView does on the cacao side.
fn background_brush(hwnd: HWND) -> HBRUSH {
    let Some(div_info) = div_state(hwnd) else {
        return HBRUSH::default();
    };
    if let Some(brush) = *div_info.background.borrow() {
        return HBRUSH(brush.0 as *mut _);
    }
    unsafe {
        match GetParent(hwnd) {
            Ok(parent) => HBRUSH(SendMessageW(parent, WM_GET_BG_BRUSH, None, None).0 as *mut _),
            Err(_) => HBRUSH::default(),
        }
    }
}

impl Div {
    /// Stretch-paint the current frame over the whole client area.
    ///
    /// StretchDIBits wants POSITIVE source dimensions as parameters — the
    /// header's negative biHeight only marks the scanline order (top-down),
    /// and passing it as nSrcHeight made GDI reject the blit silently,
    /// which is why the image never showed.
    ///
    /// Alpha is ignored for now (SRCCOPY copies RGB; the pixels are
    /// 0xAARRGGBB premultiplied) — swapping in AlphaBlend is the next step
    /// of the image work.
    fn paint_image(&self, hdc: HDC, window_rect: &RECT) {
        let source = self.source.borrow();
        let Some(ImageSource::Frame(frame)) = source.as_ref() else {
            // File sources are decoded + cached into frames by
            // load_image_file at the macro; this arm is a guard for bare
            // path bindings that arrive as ImageSource::File
            return;
        };
        if frame.width == 0 || frame.height == 0 {
            return; // empty frame: leave the erased background showing
        }

        let (dw, dh) = (
            (window_rect.right - window_rect.left).max(1) as usize,
            (window_rect.bottom - window_rect.top).max(1) as usize,
        );

        // GDI's stretch mangles LARGE sources — probe-verified: downsizing
        // anything over ~4096px wide samples the wrong source pixels
        // (queensland.jpg came out neon with a seam; ronda.jpg at a 2x
        // ratio was fine). The reliable pair is: scale in software once per
        // (frame, box), then blit 1:1 — which is exact at any size.
        let key = (frame.pixels.as_ptr() as usize, dw, dh);
        let mut scaled = self.scaled.borrow_mut();
        if scaled.as_ref().map(|(k, _)| *k) != Some(key) {
            *scaled = Some((
                key,
                scale_bgra(&frame.pixels, frame.width, frame.height, dw, dh),
            ));
        }
        let pixels = &scaled.as_ref().expect("just set").1;

        let mut bitmap_info = BITMAPINFO::default();
        bitmap_info.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
        bitmap_info.bmiHeader.biWidth = dw as i32;
        bitmap_info.bmiHeader.biHeight = -(dh as i32); // top-down scanlines
        bitmap_info.bmiHeader.biPlanes = 1;
        bitmap_info.bmiHeader.biBitCount = 32;
        bitmap_info.bmiHeader.biCompression = BI_RGB.0;

        unsafe {
            StretchDIBits(
                hdc,
                0,
                0,
                dw as i32,
                dh as i32,
                0,
                0,
                dw as i32,
                dh as i32,
                Some(pixels.as_ptr() as *const _),
                &bitmap_info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }
}

/// Scale premultiplied BGRA pixels to the box — box-average down (good
/// quality, one pass), nearest up (the `.max(x0 + 1)` spans collapse to one
/// pixel). Channel-agnostic: each byte lane is averaged on its own, so the
/// premultiplied contract survives untouched.
fn scale_bgra(src: &[Pixel], w: usize, h: usize, dw: usize, dh: usize) -> Vec<Pixel> {
    let mut out = vec![Pixel(0); dw * dh];
    for dy in 0..dh {
        let y0 = dy * h / dh;
        let y1 = ((dy + 1) * h / dh).max(y0 + 1).min(h);
        for dx in 0..dw {
            let x0 = dx * w / dw;
            let x1 = ((dx + 1) * w / dw).max(x0 + 1).min(w);
            let (mut a, mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = src[y * w + x].0 as u64;
                    a += p >> 24;
                    r += (p >> 16) & 0xff;
                    g += (p >> 8) & 0xff;
                    b += p & 0xff;
                    n += 1;
                }
            }
            out[dy * dw + dx] = Pixel(
                (((a / n) << 24) | ((r / n) << 16) | ((g / n) << 8) | (b / n)) as u32,
            );
        }
    }
    out
}

/// The `wo_div` class proc: a Div paints its own background and lends its
/// brush to children drawn on it. (Themed pushbuttons ignore
/// WM_CTLCOLORBTN — our manifest-less classic controls honor it.)
unsafe extern "system" fn divproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_NCCREATE => {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            }
            WM_COMMAND => {
                // child controls report here: LOWORD(wparam) is the control id
                // (== dispatch id), HIWORD the notification code
                //let id = (wparam.0 & 0xffff) as usize; // if we need this for something other than a button click
                let code = ((wparam.0 >> 16) & 0xffff) as u32;
                if code == BN_CLICKED || code == EN_CHANGE {
                    // button clicks bubble up to the window
                    if let Ok(parent_hwnd) = GetParent(hwnd) {
                        return SendMessageW(parent_hwnd, WM_COMMAND, Some(wparam), Some(lparam));
                    }
                }
            }
            WM_NOTIFY => {
                // notifications bubble too — and custom draw's return value
                // must propagate with them
                if let Ok(parent_hwnd) = GetParent(hwnd) {
                    return SendMessageW(parent_hwnd, WM_NOTIFY, Some(wparam), Some(lparam));
                }
            }
            WM_ERASEBKGND => {
                let hdc = HDC(wparam.0 as *mut _);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                FillRect(hdc, &rect, background_brush(hwnd));
                return LRESULT(1);
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT => {
                // children drawn on this div blend into its background
                let hdc = HDC(wparam.0 as *mut _);
                SetBkMode(hdc, TRANSPARENT);
                return LRESULT(background_brush(hwnd).0 as isize);
            }
            WM_GET_BG_BRUSH => {
                return LRESULT(background_brush(hwnd).0 as isize);
            }
            WM_PAINT => {
                if let Some(div_info) = div_state(hwnd)
                    && div_info.source.borrow().is_some()
                {
                    let dc = dc::DeviceContext::begin_paint(hwnd);
                    let mut client_rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut client_rect);
                    div_info.paint_image(dc.hdc, &client_rect);
                    // painted ourselves — don't let DefWindowProc do a
                    // second BeginPaint/EndPaint pass over it
                    return LRESULT(0);
                }
            }
            WM_SIZE => {
                // tell the root this div resized — POSTED, because child
                // divs resize from our own arrange() mid-render, and a
                // synchronous send would land while the render borrow is
                // held. wParam = our hwnd, lParam = the new client size.
                let root = GetAncestor(hwnd, GET_ANCESTOR_FLAGS(2)); // GA_ROOT
                if !root.0.is_null() {
                    let _ =
                        PostMessageW(Some(root), WM_DIV_RESIZED, WPARAM(hwnd.0 as usize), lparam);
                }
            }
            WM_NCDESTROY => {
                // the Rc<Div> arrived as lpParam via Box::into_raw — free
                // it (read the pointer BEFORE clearing it!)
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Rc<Div>;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                if !ptr.is_null() {
                    drop(Box::from_raw(ptr));
                }
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

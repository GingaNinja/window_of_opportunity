use super::get_utf16_vec;
use windows::Win32::{Foundation::*, Graphics::Gdi::*};

pub struct DeviceContext {
    hdc: HDC,
    ps: Option<PAINTSTRUCT>,
    hwnd: HWND,
    tabs: Vec<i32>,
}

impl DeviceContext {
    pub fn get_dc(hwnd: HWND) -> Self {
        let hdc: HDC;
        unsafe {
            hdc = GetDC(hwnd);
        }
        DeviceContext {
            hwnd: hwnd,
            hdc: hdc,
            ps: None,
            tabs: Vec::new(),
        }
    }

    pub fn set_tabs(&mut self, tabs: Vec<i32>) {
        self.tabs = tabs;
    }

    pub fn begin_paint(hwnd: HWND) -> Self {
        let mut ps: PAINTSTRUCT = PAINTSTRUCT::default();
        let hdc: HDC;
        unsafe {
            hdc = BeginPaint(hwnd, &mut ps);
        }
        DeviceContext {
            hdc: hdc,
            ps: Some(ps),
            hwnd,
            tabs: Vec::new(),
        }
    }

    pub fn set_pixel(&self, x: i32, y: i32, hex_color: u32) -> bool {
        unsafe { SetPixelV(self.hdc, x, y, COLORREF(hex_color)).as_bool() }
    }

    pub fn set_pen_color(&self, hex_color: u32) {
        unsafe {
            SetDCPenColor(self.hdc, COLORREF(hex_color));
        }
    }

    pub fn set_background_color(&self, hex_color: u32) {
        unsafe {
            SetBkColor(self.hdc, COLORREF(hex_color));
        }
    }

    pub fn set_background_mode(&self, mode: BACKGROUND_MODE) {
        unsafe {
            SetBkMode(self.hdc, mode);
        }
    }

    pub fn draw_text(&self, text: &str, rect: &mut RECT) {
        unsafe {
            DrawTextW(
                self.hdc,
                &mut get_utf16_vec(text)[..],
                rect,
                DT_SINGLELINE | DT_CENTER | DT_VCENTER,
            );
        }
    }

    pub fn move_to(&self, x: i32, y: i32) -> bool {
        unsafe { MoveToEx(self.hdc, x, y, None).as_bool() }
    }

    pub fn line_to(&self, x: i32, y: i32) -> bool {
        unsafe { LineTo(self.hdc, x, y).as_bool() }
    }

    pub fn polyline(&self, points: &[POINT]) -> bool {
        unsafe { Polyline(self.hdc, points).as_bool() }
    }

    pub fn rectangle(&self, l: i32, t: i32, r: i32, b: i32) -> bool {
        unsafe { Rectangle(self.hdc, l, t, r, b).as_bool() }
    }

    pub fn ellipse(&self, l: i32, t: i32, r: i32, b: i32) -> bool {
        unsafe { Ellipse(self.hdc, l, t, r, b).as_bool() }
    }

    pub fn round_rect(&self, l: i32, t: i32, r: i32, b: i32, x_corn: i32, y_corn: i32) -> bool {
        unsafe { RoundRect(self.hdc, l, t, r, b, x_corn, y_corn).as_bool() }
    }

    pub fn poly_bezier(&self, points: &[POINT]) -> bool {
        unsafe { PolyBezier(self.hdc, points).as_bool() }
    }

    pub fn select_object(&self, obj: HGDIOBJ) {
        unsafe {
            SelectObject(self.hdc, obj);
        }
    }

    pub fn get_stock_object(&self, i: GET_STOCK_OBJECT_FLAGS) -> HGDIOBJ {
        unsafe { GetStockObject(i) }
    }

    pub fn text_out(&self, text: &str, x: i32, y: i32) {
        unsafe {
            TabbedTextOutW(
                self.hdc,
                x,
                y,
                &mut get_utf16_vec(text)[..],
                Some(&self.tabs[..]),
                0,
            );
        }
    }

    pub fn text_metrics(&self) -> TEXTMETRICW {
        let mut tm = TEXTMETRICW::default();
        unsafe {
            let _ = GetTextMetricsW(self.hdc, &mut tm);
        }
        tm
    }
}

impl Drop for DeviceContext {
    fn drop(&mut self) {
        unsafe {
            match self.ps {
                None => {
                    ReleaseDC(self.hwnd, self.hdc);
                }
                Some(ps) => {
                    let _ = EndPaint(self.hwnd, &ps);
                }
            };
        }
    }
}

pub struct Pen {
    style: PEN_STYLE,
    color: u32,
    width: i32,
    pub handle: HPEN,
}

impl Pen {
    pub fn new(style: PEN_STYLE, color: u32, width: i32) -> Self {
        Self {
            style,
            color,
            width,
            handle: Self::create_pen(style, width, color),
        }
    }

    pub fn create_pen(style: PEN_STYLE, width: i32, hex_color: u32) -> HPEN {
        unsafe { CreatePen(style, width, COLORREF(hex_color)) }
    }

    pub fn create_pen_indirect(log_pen: &mut LOGPEN) -> HPEN {
        unsafe { CreatePenIndirect(log_pen) }
    }
}

impl Drop for Pen {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.handle);
        }
    }
}

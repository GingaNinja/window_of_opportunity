use std::mem;

use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::{FillRect, HBRUSH, HDC, SetBkMode, TRANSPARENT},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::*,
};

use super::{app::WM_GET_BG_BRUSH, util::load_cursor};

const DIV_CLASS: PCWSTR = w!("wo_div");

/// A Div's child window — its own background brush rides along as
/// lpParam (null = inherit the parent's).
pub fn create_div(parent: HWND, background: Option<HBRUSH>) -> HWND {
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
            Some(background.map_or(std::ptr::null(), |brush| brush.0) as *const _),
        )
        .expect("CreateWindowExW div")
    }
}

/// The Div class: no class brush (divs paint themselves), the brush rides
/// in each window's GWLP_USERDATA (set from lpParam at creation).
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
        debug_assert!(atom != 0);
    }
}

/// The brush a div paints with: its own if it has one, else the parent's —
/// an undecorated Div shows its parent's backdrop like a transparent
/// NSView does on the cacao side.
fn background_brush(hwnd: HWND) -> HBRUSH {
    unsafe {
        let own = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if own != 0 {
            return HBRUSH(own as *mut _);
        }
        match GetParent(hwnd) {
            Ok(parent) => HBRUSH(SendMessageW(parent, WM_GET_BG_BRUSH, None, None).0 as *mut _),
            Err(_) => HBRUSH::default(),
        }
    }
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
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

//! # Window Of Opportunity
//!
//! `window_of_opportunity` is a simple (at the moment)
//! library for managing UIs. Currently it only works
//! in win32, but I plan to work with MacOS and Linux
//! in the near future.
//!
//! It will ultimately support different controls to
//! display in a window, and have a nice interface for
//! doing so, but currently it's just a blank window
//! that can be drawn on with an onPaint event handler.

use std::{any::Any, cell::RefCell, rc::Rc};

pub use self::win::Win;

use app::WPApp;
use dc::DeviceContext;
use dioxus::{
    events::{
        InteractionElementOffset, InteractionLocation, ModifiersInteraction, PointerInteraction,
    },
    html::{events::MouseData, HasMouseData, HtmlEventConverter, MountedData, PlatformEventData},
};
use win_create_args::WinCreateArgs;
use windows::{
    core::*,
    Win32::{Foundation::*, Graphics::Gdi::TEXTMETRICW, UI::WindowsAndMessaging::*},
};

use dioxus_core::{Element, ElementId, Event as DEvent, VirtualDom};

pub mod app;
pub mod components;
pub mod dc;
pub mod kbd;
pub mod module;
pub mod mouse;
pub mod win;
pub mod win_create_args;

pub fn launch(app: fn() -> Element) {
    launch_vdom(VirtualDom::new(app));
}

pub fn launch_vdom(vdom: VirtualDom) {
    let _ = render(vdom);
}

#[derive(Clone)]
struct MyMouseData {}
impl HasMouseData for MyMouseData {
    fn as_any(&self) -> &dyn std::any::Any {
        todo!()
    }
}
impl PointerInteraction for MyMouseData {
    fn trigger_button(&self) -> Option<dioxus::html::input_data::MouseButton> {
        todo!()
    }

    fn held_buttons(&self) -> dioxus::html::input_data::MouseButtonSet {
        dioxus::html::input_data::MouseButtonSet::new()
    }
}
impl ModifiersInteraction for MyMouseData {
    fn modifiers(&self) -> dioxus::prelude::Modifiers {
        todo!()
    }
}
impl InteractionElementOffset for MyMouseData {
    fn element_coordinates(&self) -> dioxus::html::geometry::ElementPoint {
        todo!()
    }
}
impl InteractionLocation for MyMouseData {
    fn client_coordinates(&self) -> dioxus::html::geometry::ClientPoint {
        todo!()
    }

    fn screen_coordinates(&self) -> dioxus::html::geometry::ScreenPoint {
        todo!()
    }

    fn page_coordinates(&self) -> dioxus::html::geometry::PagePoint {
        todo!()
    }
}

struct NativeEventConverter;
impl HtmlEventConverter for NativeEventConverter {
    fn convert_animation_data(&self, _event: &PlatformEventData) -> dioxus::prelude::AnimationData {
        todo!()
    }

    fn convert_clipboard_data(&self, _event: &PlatformEventData) -> dioxus::prelude::ClipboardData {
        todo!()
    }

    fn convert_composition_data(
        &self,
        _event: &PlatformEventData,
    ) -> dioxus::prelude::CompositionData {
        todo!()
    }

    fn convert_drag_data(&self, _event: &PlatformEventData) -> dioxus::prelude::DragData {
        todo!()
    }

    fn convert_focus_data(&self, _event: &PlatformEventData) -> dioxus::prelude::FocusData {
        todo!()
    }

    fn convert_form_data(&self, _event: &PlatformEventData) -> dioxus::prelude::FormData {
        todo!()
    }

    fn convert_image_data(&self, _event: &PlatformEventData) -> dioxus::prelude::ImageData {
        todo!()
    }

    fn convert_keyboard_data(&self, _event: &PlatformEventData) -> dioxus::prelude::KeyboardData {
        todo!()
    }

    fn convert_media_data(&self, _event: &PlatformEventData) -> dioxus::prelude::MediaData {
        todo!()
    }

    fn convert_mounted_data(&self, _event: &PlatformEventData) -> MountedData {
        todo!()
    }

    fn convert_mouse_data(&self, event: &PlatformEventData) -> MouseData {
        event.downcast::<MyMouseData>().cloned().unwrap().into()
    }

    fn convert_pointer_data(&self, _event: &PlatformEventData) -> dioxus::prelude::PointerData {
        todo!()
    }

    fn convert_resize_data(&self, _event: &PlatformEventData) -> dioxus::prelude::ResizeData {
        todo!()
    }

    fn convert_scroll_data(&self, _event: &PlatformEventData) -> dioxus::prelude::ScrollData {
        todo!()
    }

    fn convert_selection_data(&self, _event: &PlatformEventData) -> dioxus::prelude::SelectionData {
        todo!()
    }

    fn convert_toggle_data(&self, _event: &PlatformEventData) -> dioxus::prelude::ToggleData {
        todo!()
    }

    fn convert_touch_data(&self, _event: &PlatformEventData) -> dioxus::prelude::TouchData {
        todo!()
    }

    fn convert_transition_data(
        &self,
        _event: &PlatformEventData,
    ) -> dioxus::prelude::TransitionData {
        todo!()
    }

    fn convert_visible_data(&self, _event: &PlatformEventData) -> dioxus::prelude::VisibleData {
        todo!()
    }

    fn convert_wheel_data(&self, _event: &PlatformEventData) -> dioxus::prelude::WheelData {
        todo!()
    }
}

// hwnd could be used as the id. You then modify/destroy windows (controls)
//
pub fn render(mut vdom: VirtualDom) -> Result<()> {
    let create_args = WinCreateArgs {
        instance: HINSTANCE::default(),
        window_height: 400,
        window_width: 600,
        //     & !(WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE),
        ..WinCreateArgs::default_win_main()
    };

    let mut app = WPApp::new_with_config(create_args);
    dioxus::html::set_event_converter(Box::new(NativeEventConverter));
    // app.main_win.set_created_callback(move |win| {
    //     vdom.rebuild(win);
    // });
    let main_win = &mut app.main_win;
    app.main_win.set_event_callback(move |win, event| {
        match event {
            app::ReactiveEvent::Created => vdom.rebuild(win),
            app::ReactiveEvent::Command => {
                println!("received command event");
                let data = MyMouseData {};
                let data = PlatformEventData::new(Box::new(data));
                let data = Rc::new(data) as Rc<dyn Any>;
                // let pdata = data.clone().downcast::<PlatformEventData>().unwrap();
                let event = DEvent::new(data, true);
                vdom.runtime()
                    .handle_event("click", event.clone(), ElementId(2));
            }
        }
    });
    app.init(w!("testing"))?;

    app.run(|_app| {});
    Ok(())
}

pub enum SendMessageParams {
    Close,
}

/// Determines if an event handler handled the event
#[derive(PartialEq)]
pub enum EventHandled {
    Handled(LRESULT),
    NotHandled,
}

/// An Event struct is what is
/// passed as an abstraction
/// instead of the wndproc args
#[derive(Debug)]
pub struct Event {
    pub hwnd: HWND,
    pub message: u32,
    pub wparam: WPARAM,
    pub lparam: LPARAM,
}

#[derive(PartialEq, Debug)]
pub enum SourceType {
    Menu,
    Accelerator,
    Control,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct CommandEvent {
    command: i32,
    source_type: SourceType,
    control_hwnd: Option<HWND>,
}

#[macro_export]
macro_rules! ui {
    ( $el:ident
    $($title:literal)?
$( $method:ident $methodargs:tt)*
$({ $($contents:tt)* })? ) => {{
        #[allow(unused_imports)]
        use $crate::{WPApp, win_create_args::WinCreateArgs};
        use windows::{core::*, Win32::Foundation::HINSTANCE};
        let create_args = WinCreateArgs {
            instance: HINSTANCE::default(),
            $( $method: $methodargs, )*
            //     & !(WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE),
            ..WinCreateArgs::default_win_main()
        };
        let mut app = WPApp::<$el>::new_with_config(create_args);
        let mut title = PCWSTR::null();
        $( title = w!($title);)?
        if (title == PCWSTR::null()) {
            title = w!("Default");
        }
        app
    }};
}

#[macro_export]
macro_rules! default_win_impl {
    () => {
        fn get_hwnd(&self) -> HWND {
            self.base.hwnd
        }
        fn set_hwnd(&mut self, hwnd: HWND) {
            self.base.hwnd = hwnd;
        }

        fn to_self_ptr(ptr: *mut ::core::ffi::c_void) -> *mut Self {
            ptr as *mut Self
        }
        //    ptr_self as _
        fn raw_ptr_isize(raw_ptr: *mut Self) -> isize {
            raw_ptr as _
        }

        fn get_canary(&self) -> i32 {
            self.base.canary
        }

        fn get_base(&self) -> &BaseWin {
            &self.base
        }
    };
}

#[derive(Debug)]
pub struct BaseWin {
    pub hwnd: HWND,
    pub canary: i32,
    pub tm: RefCell<TEXTMETRICW>,
    pub x: RefCell<i32>,
    pub y: RefCell<i32>,
    pub left: RefCell<i32>,
    pub top: RefCell<i32>,
    // cx_char: i32,
    // cx_caps: i32,
    // cy_char: i32,
    // max_width: i32,
}

impl Default for BaseWin {
    fn default() -> Self {
        Self {
            hwnd: HWND::default(),
            canary: 99,
            tm: RefCell::new(TEXTMETRICW::default()),
            x: RefCell::new(0),
            y: RefCell::new(0),
            left: RefCell::new(0),
            top: RefCell::new(0),
        }
    }
}

impl BaseWin {
    pub fn on_create(&self, _event: &Event) {
        let dc = DeviceContext::get_dc(self.hwnd);

        *self.tm.borrow_mut() = dc.text_metrics();
    }
    //     let hdc = get_dc(event.hwnd);
    //     let mut tm = TEXTMETRICW::default();

    //     get_text_metrics(hdc, &mut tm);
    //     self.cx_char = tm.tmAveCharWidth;
    //     self.cy_char = tm.tmHeight + tm.tmExternalLeading;
    //     self.cx_caps = if tm.tmPitchAndFamily & TMPF_FLAGS(1) == TMPF_FLAGS(1) {
    //         3
    //     } else {
    //         2
    //     } * self.cx_char
    //         / 2;

    //     release_dc(event.hwnd, hdc);

    //     self.max_width = 40 * self.cx_char + 22 * self.cx_caps;

    //     EventHandled::Handled(LRESULT(0))
    // }

    pub fn on_resize(&self, x: i32, y: i32) -> EventHandled {
        *self.x.borrow_mut() = x;
        *self.y.borrow_mut() = y;
        // let si = SCROLLINFO {
        //     cbSize: mem::size_of::<SCROLLINFO>() as u32,
        //     fMask: SIF_RANGE | SIF_PAGE,
        //     nMin: 0,
        //     // nMax: sys_metrics.len() as i32 - 1,
        //     nMax: self.text_out.len() as i32 - 1,
        //     nPage: self.cy_client as u32 / self.cy_client as u32,
        //     ..Default::default()
        // };

        // set_scroll_info(event.hwnd, SB_VERT, &si, TRUE);
        // let si = SCROLLINFO {
        //     cbSize: mem::size_of::<SCROLLINFO>() as u32,
        //     fMask: SIF_RANGE | SIF_PAGE,
        //     nMin: 0,
        //     nMax: 2 + self.max_width / self.cx_char,
        //     nPage: self.cx_client as u32 / self.cx_char as u32,
        //     ..Default::default()
        // };
        // set_scroll_info(event.hwnd, SB_HORZ, &si, TRUE);
        EventHandled::Handled(LRESULT(0))
    }

    // fn on_paint(&self, event: &window::Event) -> window::EventHandled {
    //     let mut ps: PAINTSTRUCT = PAINTSTRUCT::default();
    //     let hdc = begin_paint(event.hwnd, &mut ps);

    //     let mut si = SCROLLINFO {
    //         cbSize: mem::size_of::<SCROLLINFO>() as u32,
    //         fMask: SIF_POS,
    //         ..Default::default()
    //     };
    //     let _ = get_scroll_info(event.hwnd, SB_VERT, &mut si);
    //     let i_vert_pos = si.nPos;
    //     let _ = get_scroll_info(event.hwnd, SB_HORZ, &mut si);
    //     let i_horz_pos = si.nPos;

    //     let i_paint_begin = cmp::max(0, i_vert_pos + ps.rcPaint.top / self.cy_char);
    //     let i_paint_end = cmp::min(
    //         self.text_out.len() as i32 - 1,
    //         i_vert_pos + ps.rcPaint.bottom / self.cy_char,
    //     );

    //     for (i, line) in self.text_out.iter().enumerate() {
    //         if (i as i32) < i_paint_begin || (i as i32) > i_paint_end {
    //             continue;
    //         }
    //         let x = self.cx_char * (1 - i_horz_pos);
    //         let y = self.cy_char * (i as i32 - i_vert_pos);
    //         text_out(hdc, x, y, line);
    //     }

    //     end_paint(event.hwnd, &ps);
    //     window::EventHandled::Handled(LRESULT(0))
    // }
}

// pub struct WPWin {
//     hwnd: HWND,
//     my_num: i32,
// }

// impl WPWin {
//     pub fn new() -> Self {
//         WPWin {
//             hwnd: HWND(0),
//             my_num: 44,
//         }
//     }
// }

// impl Win for WPWin {
//     fn get_hwnd(&self) -> HWND {
//         self.hwnd
//     }
//     fn set_hwnd(&mut self, hwnd: HWND) {
//         self.hwnd = hwnd;
//     }

//     fn to_self_ptr(ptr: *mut ::core::ffi::c_void) -> *mut Self {
//         ptr as *mut Self
//     }
//     //    ptr_self as _
//     fn raw_ptr_isize(raw_ptr: *mut Self) -> isize {
//         raw_ptr as _
//     }

//     fn on_create(&self, event: &Event) -> EventHandled {
//         println!("my_num is {}", self.my_num);
//         EventHandled::Handled(LRESULT(0))
//     }
// }

// pub fn set_scroll_info(hwnd: HWND, sb_option: SCROLLBAR_CONSTANTS, si: &SCROLLINFO, redraw: BOOL) {
//     unsafe {
//         SetScrollInfo(hwnd, sb_option, si, redraw);
//     }
// }

// pub fn get_scroll_info(hwnd: HWND, sb_option: SCROLLBAR_CONSTANTS, si: &mut SCROLLINFO) {
//     unsafe {
//         GetScrollInfo(hwnd, sb_option, si);
//     }
// }

// pub fn text_out(hdc: HDC, x: i32, y: i32, text: &str) {
//     unsafe {
//         TextOutW(hdc, x, y, &mut get_utf16_vec(text)[..]);
//     }
// }

pub fn load_icon(inst: HINSTANCE, name: PCWSTR) -> Result<HICON> {
    match hword(name.0 as isize) {
        0 => unsafe { LoadIconW(None, name) },
        _ => unsafe { LoadIconW(inst, name) },
    }
}

pub fn load_cursor(inst: Option<HINSTANCE>, name: PCWSTR) -> Result<HCURSOR> {
    match inst {
        None => unsafe { LoadCursorW(None, name) },
        Some(inst) => unsafe { LoadCursorW(inst, name) },
    }
}
fn get_utf16_vec(text: &str) -> Vec<u16> {
    let mut text: Vec<u16> = text.encode_utf16().collect();
    text.push(0);

    text
}

/// Get the lower word from a 64 bit word. Note this
/// will not be public ultimately, as it's really
/// just a utility function
///
/// # Examples
///
/// ```
/// let val = 0xf0f00f0f;
/// let expected = 0x0f0f;
///
/// assert_eq!(window_of_opportunity::lword(val), expected);
/// ```
///
/// # Panics
///
/// This is just to put a placeholder here
/// but we could probably put code in
/// ```
/// let code = "blah";
/// ```
///
/// # Errors
///
/// And errors are of this type...
///
/// # Safety
///
/// If it's unsafe.
pub fn lword(val: isize) -> i32 {
    (val & 0xffff) as i32
}

/// Get the high word from a 64 bit word. Note this
/// will not be public ultimately, as it's really
/// just a utility function
///
/// # Examples
///
/// ```
/// let val = 0xf0f00f0f;
/// let expected = 0xf0f0;
///
/// assert_eq!(window_of_opportunity::hword(val), expected);
/// ```
pub fn hword(val: isize) -> i32 {
    ((val >> 16) & 0xffff) as i32
}

#[cfg(test)]
mod tests {}

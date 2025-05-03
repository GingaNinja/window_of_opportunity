use crate::dc::DeviceContext;
use crate::kbd::KbdEvent;
use crate::mouse::MouseEvent;
use crate::win::{Component, Container};
use crate::{
    default_win_impl, hword, lword, BaseWin, CommandEvent, Event, EventHandled, SourceType, Win,
    WinCreateArgs,
};
use std::mem;
use windows::{
    core::*,
    Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
};

pub struct Div {
    base: BaseWin,
    inst: HINSTANCE,
    child: Option<Component>,
    created: bool,
    created_callback: Option<Box<dyn FnMut(&mut Div) -> ()>>,
    hwnd: HWND,
}

impl Container for Div {
    fn create_container(&mut self, parent: HWND, instance: HINSTANCE) -> Result<()> {
        unsafe {
            match CreateWindowExW(
                WS_EX_LEFT,
                w!("div"),
                w!("div"),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                10,
                10,
                100,
                20,
                parent,
                HMENU(999 as *mut std::ffi::c_void),
                instance,
                Some(self as *const _ as _),
            ) {
                Ok(hwnd) => {
                    self.hwnd = hwnd;
                    Ok(())
                }
                Err(err) => Err(err),
            }
        }
    }

    fn add_child(&mut self, child: Component) {
        self.child = Some(child);
    }
}

impl Div {
    default_win_impl!();
    pub fn new(inst: HINSTANCE) -> Self {
        let mut div = Div {
            base: BaseWin::default(),
            inst,
            child: None,
            created: false,
            created_callback: None,
            hwnd: HWND::default(),
        };

        div.create_component();
        div
    }
    fn set_child(&mut self, mut child: Component) {
        if self.created {
            child.create_element(self.get_hwnd(), self.inst).unwrap();
        }
        self.child = Some(child);
    }

    fn create_component(&mut self) -> Result<()> {
        let create_args = WinCreateArgs {
            instance: self.inst.into(),
            ..WinCreateArgs::default_win_main()
        };
        self.create_comp(&create_args)
    }

    fn create_comp(&mut self, create_args: &WinCreateArgs) -> std::result::Result<(), Error> {
        let brush: HGDIOBJ;
        unsafe {
            brush = GetStockObject(BLACK_BRUSH);
        }
        let brush = HBRUSH(brush.0);

        let icon = HICON::default();
        let wc = WNDCLASSEXW {
            hCursor: create_args.cursor,
            hIcon: icon,
            hInstance: create_args.instance,
            lpszClassName: w!("div"),
            hbrBackground: brush,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(Self::wndproc),
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            lpszMenuName: create_args.menu_name,
            ..Default::default()
        };

        unsafe {
            // if class doesn't already exist? check the result for this...
            let atom = RegisterClassExW(&wc);
            debug_assert!(atom != 0);
        }
        Ok(())
    }

    fn on_command(&self, event: &CommandEvent) -> EventHandled {
        println!("command... {:?}", event);
        match event.command {
            100 => {
                //self.send_message(SendMessageParams::Close);
                EventHandled::Handled(LRESULT(0))
            }
            _ => EventHandled::NotHandled,
        }
    }

    fn get_client_rect(&self) -> Result<RECT> {
        let mut rect = RECT::default();
        unsafe {
            GetClientRect(self.get_hwnd(), &mut rect)?;
        }
        Ok(rect)
    }

    fn on_paint(&self, _hdc: &mut DeviceContext, _rect: &mut RECT) -> EventHandled {
        EventHandled::NotHandled
    }

    fn on_create(&mut self, _event: &Event) -> EventHandled {
        EventHandled::NotHandled
    }

    fn on_resize(&mut self, _x: i32, _y: i32) -> EventHandled {
        EventHandled::NotHandled
    }

    fn on_destroy(&self, _event: &Event) -> EventHandled {
        EventHandled::NotHandled
    }

    fn on_ncdestroy(&self, _event: &Event) -> EventHandled {
        EventHandled::NotHandled
    }

    fn on_mouse(&self, _event: &MouseEvent) -> EventHandled {
        EventHandled::NotHandled
    }

    fn on_kbd(&self, _event: &KbdEvent) -> EventHandled {
        EventHandled::NotHandled
    }

    fn dispatch_event(&mut self, event: &Event) -> LRESULT {
        if self.get_canary() != 99 {
            println!("error: canary is not 99!");
            return LRESULT(1);
        }
        let processed_event = match event.message {
            WM_CREATE => {
                self.get_base().on_create(event);
                self.on_create(event)
            }
            WM_PAINT => {
                let mut hdc = DeviceContext::begin_paint(self.get_hwnd());
                let handled = match self.get_client_rect() {
                    Err(err) => {
                        println!("error getting client rect: {:?}", err);
                        EventHandled::NotHandled
                    }
                    Ok(mut rect) => self.on_paint(&mut hdc, &mut rect),
                };
                handled
                // let mut ps: PAINTSTRUCT = PAINTSTRUCT::default();
                // let hdc: HDC;
                // unsafe {
                //     hdc = BeginPaint(event.hwnd, &mut ps);
                // }

                // // println!("my_num {:?}", win_obj.my_num);
                // // println!("hwnd: {:?}", win_obj.get_hwnd());

                // // DrawTextW(hdc, text, &mut rect, DT_SINGLELINE | DT_CENTER | DT_VCENTER);
                // unsafe {
                //     EndPaint(event.hwnd, &ps);
                // }
            }
            WM_SIZE => {
                let x = lword(event.lparam.0);
                let y = hword(event.lparam.0);
                self.get_base().on_resize(x, y);
                self.on_resize(x, y)
            }
            WM_DESTROY => self.on_destroy(event),
            WM_NCDESTROY => self.on_ncdestroy(event),
            WM_COMMAND => {
                let command_type = match hword(event.wparam.0 as isize) {
                    0 => SourceType::Menu,
                    1 => SourceType::Accelerator,
                    _ => SourceType::Control,
                };
                let command_event = CommandEvent {
                    command: lword(event.wparam.0 as isize),
                    control_hwnd: if command_type == SourceType::Control {
                        Some(HWND(event.lparam.0 as *mut std::ffi::c_void))
                    } else {
                        None
                    },
                    source_type: command_type,
                };
                self.on_command(&command_event)
            }
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN
            | WM_RBUTTONUP | WM_RBUTTONDBLCLK | WM_MBUTTONDBLCLK | WM_MBUTTONDOWN
            | WM_MBUTTONUP => self.on_mouse(&MouseEvent::new(event)),
            WM_KEYDOWN | WM_KEYUP | WM_CHAR | WM_DEADCHAR => self.on_kbd(&KbdEvent::new(event)),
            _ => EventHandled::NotHandled,
        };

        if event.message == WM_NCDESTROY {
            println!("WM_NCDESTROY");
            unsafe {
                SetWindowLongPtrW(event.hwnd, GWLP_USERDATA, 0);
            }
            self.set_hwnd(HWND::default());
            return unsafe {
                DefWindowProcW(event.hwnd, event.message, event.wparam, event.lparam)
            };
        }

        match processed_event {
            EventHandled::NotHandled => unsafe {
                DefWindowProcW(event.hwnd, event.message, event.wparam, event.lparam)
            },
            EventHandled::Handled(lresult) => lresult,
        }
    }

    extern "system" fn wndproc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            if hwnd == HWND::default() {
                return DefWindowProcW(hwnd, message, wparam, lparam);
            }
        }

        let ptr_self = match message {
            WM_NCCREATE => {
                println!("NC Create");
                unsafe {
                    let createstruct = &mut *(lparam.0 as *mut CREATESTRUCTW);

                    println!("nc create raw_ptr: {:?}", createstruct.lpCreateParams);
                    let ptr_self = Self::to_self_ptr(createstruct.lpCreateParams);

                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Self::raw_ptr_isize(ptr_self));
                    let ref_self = &mut *ptr_self;

                    // println!("my_num in wndproc: {:?}", ref_self.my_num);
                    ref_self.set_hwnd(hwnd.clone());

                    // let window_obj = Rc::from_raw(createstruct.lpCreateParams as *const &WPWin);
                    // println!("window (in proc) hwnd: {:?}", window_obj.get_hwnd());
                    // println!("proc hwnd: {:?}", window);
                    // println!("my_num: {:?}", window_obj.my_num);
                    ptr_self
                }
                //LRESULT(1)
            }
            _ => unsafe {
                Self::to_self_ptr(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ::core::ffi::c_void)
            },
        };

        if ptr_self.is_null() {
            println!("ptr_self is null");
            unsafe {
                return DefWindowProcW(hwnd, message, wparam, lparam);
            }
        }
        let ref_self: &mut Self;
        unsafe {
            ref_self = &mut *ptr_self;
        }

        let event = super::Event {
            hwnd: hwnd,
            message: message,
            wparam: wparam,
            lparam: lparam,
        };
        ref_self.dispatch_event(&event)
    }
}

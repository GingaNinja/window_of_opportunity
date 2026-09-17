use crate::app::ReactiveEvent;
use crate::dc::DeviceContext;
use crate::kbd::KbdEvent;
use crate::mouse::MouseEvent;
use crate::win::{Component, Observer};
use crate::{
    default_win_impl, hword, lword, BaseWin, CommandEvent, Event, EventHandled, SourceType,
    WinCreateArgs,
};
use std::cell::RefCell;
use std::ffi::c_void;
use std::fmt::Debug;
use std::mem;
use std::rc::{Rc, Weak};
use windows::Win32::System::WindowsProgramming::MulDiv;
use windows::{
    core::*,
    Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
};

pub trait CustomComponent {
    fn to_self_ptr(c_void: *mut ::core::ffi::c_void) -> *mut Self;
    fn get_canary(&self) -> i32 {
        10
    }
    fn get_hwnd(&self) -> HWND;
    fn set_hwnd(&mut self, hwnd: HWND);
    fn get_base(&mut self) -> &mut BaseWin;
    fn set_window_pos(&mut self, x: i32, y: i32, width: i32, height: i32) {
        unsafe {
            SetWindowPos(self.get_hwnd(), HWND_TOP, x, y, width, height, SWP_NOZORDER).unwrap();
        }
    }
    fn raw_ptr_isize(ptr: *mut Self) -> isize;
    fn create_comp(&mut self, create_args: &WinCreateArgs) -> std::result::Result<(), Error> {
        let brush: HGDIOBJ;
        unsafe {
            brush = GetStockObject(create_args.brush);
        }
        let brush = HBRUSH(brush.0);

        let icon = HICON::default();
        let wc = WNDCLASSEXW {
            hCursor: create_args.cursor,
            hIcon: icon,
            hInstance: create_args.instance,
            lpszClassName: create_args.class_name,
            hbrBackground: brush,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(Self::wndproc),
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            lpszMenuName: create_args.menu_name,
            ..Default::default()
        };

        unsafe {
            // if class doesn't already exist? check the result for this...
            let _atom = RegisterClassExW(&wc);
            //            debug_assert!(atom != 0);
        }
        Ok(())
    }

    fn on_command(self: Rc<Self>, event: &CommandEvent) -> EventHandled {
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

    fn on_paint(&mut self, _hdc: &mut DeviceContext, _rect: &mut RECT) -> EventHandled {
        EventHandled::NotHandled
    }

    fn on_erasebkgnd(&mut self, _hdc: &mut DeviceContext, _rect: &mut RECT) -> EventHandled {
        EventHandled::NotHandled
    }

    fn on_create(self: Rc<Self>, _event: &Event) -> EventHandled {
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

    fn dispatch_event(&self, event: &Event) -> LRESULT {
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
            WM_ERASEBKGND => {
                let mut hdc =
                    DeviceContext::new(HDC(event.wparam.0 as *mut c_void), self.get_hwnd());
                match self.get_client_rect() {
                    Err(err) => {
                        println!("error getting client rect: {:?}", err);
                        EventHandled::NotHandled
                    }
                    Ok(mut rect) => self.on_erasebkgnd(&mut hdc, &mut rect),
                }
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

                    ref_self.set_hwnd(hwnd.clone());

                    ptr_self
                }
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
            hwnd,
            message,
            wparam,
            lparam,
        };
        ref_self.dispatch_event(&event)
    }
}

pub struct Div {
    base: BaseWin,
    inst: HINSTANCE,
    pub children: Vec<Rc<dyn Component>>,
    created: RefCell<bool>,
    observers: RefCell<Vec<Weak<dyn Observer>>>,
    hwnd: HWND,
    bk_color: u32,
    bk_brush: HBRUSH,
}

impl Debug for Div {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Div<'a> {
            base: &'a BaseWin,
            inst: &'a HINSTANCE,
            children: &'a Vec<Box<dyn Component>>,
            created: &'a bool,
            hwnd: &'a HWND,
            bk_color: &'a u32,
        }

        let Self {
            base,
            inst,
            children,
            created,
            hwnd,
            bk_color,
            bk_brush: _,
            observers: _,
        } = self;

        std::fmt::Debug::fmt(
            &Div {
                base,
                inst,
                children,
                created,
                hwnd,
                bk_color,
            },
            f,
        )
    }
}

impl CustomComponent for Div {
    default_win_impl!();
    fn on_create(self: Rc<Self>, _event: &Event) -> EventHandled {
        let self_ref = self.clone();
        let mut created = self_ref.created.borrow_mut();
        *created = true;
        let x = self.clone().get_base().x;
        let y = self.get_base().y;
        let hwnd = self.get_hwnd();
        for i in 0..self.children.len() {
            let rect = RECT {
                left: 0,
                top: 0,
                right: x,
                bottom: y,
            };
            // println!("rect: {rect:?}");

            let child = &self.children[i];
            child.create_element(hwnd, self.inst, &rect).unwrap();
        }
        println!("created div...");
        self.send_to_observers(ReactiveEvent::Created);
        EventHandled::Handled(LRESULT(0))
    }

    fn on_command(self: Rc<Self>, event: &CommandEvent) -> EventHandled {
        println!("button clicked in div");
        self.send_to_observers(ReactiveEvent::Command);
        EventHandled::Handled(LRESULT(0))
    }

    fn on_erasebkgnd(&mut self, hdc: &mut DeviceContext, rect: &mut RECT) -> EventHandled {
        hdc.fill_rect(&rect, self.bk_brush);
        EventHandled::Handled(LRESULT(1))
    }
}

impl Component for Div {
    fn create_element(
        &mut self,
        parent: HWND,
        instance: HINSTANCE,
        parent_rect: &RECT,
    ) -> Result<HWND> {
        unsafe {
            match CreateWindowExW(
                WS_EX_LEFT,
                w!("div"),
                w!("div"),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                parent_rect.left,
                parent_rect.top,
                parent_rect.right - parent_rect.left,
                parent_rect.bottom - parent_rect.top,
                parent,
                HMENU(999 as *mut std::ffi::c_void),
                instance,
                Some(self as *const _ as _),
            ) {
                Ok(hwnd) => {
                    self.hwnd = hwnd;
                    Ok(hwnd)
                }
                Err(err) => Err(err),
            }
        }
    }

    fn get_child(&self, i: usize) -> Rc<dyn Component> {
        // panic!("doesn't work");
        self.children[i].clone()
    }

    // fn set_event_callback(&mut self, callback: Box<dyn Fn(ReactiveEvent)) {
    //     self.event_callback = Some(Box::new(RefCell::new(callback)));
    // }
    fn swap_node_with_nodes(&mut self, index: usize, mut nodes: Vec<Rc<dyn Component>>) {
        let item = nodes.remove(0);
        // let (item, rest) = nodes.split_first().unwrap();
        self.children.push(item);
        self.children.swap_remove(index);
        let mut i = index + 1;
        while !nodes.is_empty() {
            let item = nodes.remove(0);
            self.children.insert(i, item);
            i += 1;
        }
    }

    // fn insert(&mut self, Vec<Box<dyn Component>>) {
    //     self.children.insert(index, element);
    // }

    fn update_dpi(&mut self, dpi: u32) {
        for i in 0..self.children.len() {
            self.children[i].clone().update_dpi(dpi);
        }
    }

    fn set_window_position(&mut self, x: i32, y: i32, width: i32, _height: i32) {
        let mut child_y = 0;
        for child in &mut self.children {
            child.set_window_position(x, child_y, width, 16);
            let (_, actual_y) = child.get_dimensions();
            child_y += actual_y;
        }

        self.set_window_pos(x, y, width, child_y);
    }

    fn get_dimensions(&self) -> (i32, i32) {
        (self.base.x, self.base.y)
    }
}

impl Div {
    pub fn new(inst: HINSTANCE) -> Self {
        let mut div = Div {
            base: BaseWin::default(),
            inst,
            children: vec![],
            created: false,
            bk_color: 0x00FFFFFF,
            bk_brush: HBRUSH::default(),
            created_callback: None,
            event_callback: None,
            hwnd: HWND::default(),
        };
        div.set_bk_colour(0x00FFFFFF);

        let _ = div.create_component();
        div
    }

    fn send_to_observers(self, event: ReactiveEvent) {
        let observers = self.observers.borrow();
        for observer in observers.iter() {
            if let Some(observer) = observer.upgrade() {
                println!("calling callback...");
                observer.notify(event.clone());
            }
        }
    }
    pub fn set_bk_colour(&mut self, hex: u32) {
        let brush;
        unsafe {
            let colorref = COLORREF(hex);
            brush = CreateSolidBrush(colorref);
        }
        // let brush = HBRUSH(brush.0);
        self.bk_brush = brush;
        self.bk_color = hex;
    }

    fn create_component(&mut self) -> Result<()> {
        let create_args = WinCreateArgs {
            instance: self.inst.into(),
            class_name: w!("div"),
            brush: GET_STOCK_OBJECT_FLAGS::default(),
            ..WinCreateArgs::default()
        };
        self.create_comp(&create_args)
    }
    pub fn add_child(&mut self, mut child: Box<dyn Component>) {
        println!("setting callback in div");
        child.set_event_callback(Box::new(|event| {
            let callback = self.event_callback.as_ref();
            if let Some(callback) = callback {
                (callback.borrow_mut())(event);
            }
            println!("event passed to div");
        }));
        if self.created {
            child
                .create_element(
                    self.get_hwnd(),
                    self.inst,
                    &RECT {
                        left: 0,
                        right: 0,
                        top: 0,
                        bottom: 0,
                    },
                )
                .unwrap();
        }
        self.children.push(child);
    }
}

#[derive(Debug)]
pub struct Text {
    base: BaseWin,
    inst: HINSTANCE,
    text: String,
    hwnd: HWND,
    actual_x: i32,
    actual_y: i32,
    font: HFONT,
    actual_font_size: i32,
}

impl CustomComponent for Text {
    default_win_impl!();

    fn on_paint(&mut self, hdc: &mut DeviceContext, rect: &mut RECT) -> EventHandled {
        hdc.select_object(HGDIOBJ::from(self.font));
        hdc.draw_text(&self.text, rect);
        EventHandled::Handled(LRESULT(0))
    }

    fn on_erasebkgnd(&mut self, _hdc: &mut DeviceContext, _rect: &mut RECT) -> EventHandled {
        EventHandled::Handled(LRESULT(0))
    }

    fn on_create(&mut self, _event: &Event) -> EventHandled {
        let h_font;
        unsafe {
            h_font = CreateFontW(
                self.actual_font_size,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                0,
                w!("Segoe UI"),
            );
        }
        self.font = h_font;
        EventHandled::NotHandled
    }
}

impl Component for Text {
    fn create_element(
        &mut self,
        parent: HWND,
        instance: HINSTANCE,
        parent_rect: &RECT,
    ) -> Result<HWND> {
        unsafe {
            match CreateWindowExW(
                WS_EX_LEFT,
                w!("text"),
                w!("text"),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_PUSHBUTTON as u32),
                parent_rect.left,
                parent_rect.top,
                parent_rect.right - parent_rect.left,
                parent_rect.bottom - parent_rect.top,
                parent,
                HMENU(888 as *mut std::ffi::c_void),
                instance,
                Some(self as *const _ as _),
            ) {
                Ok(hwnd) => {
                    self.hwnd = hwnd;
                    Ok(hwnd)
                }
                Err(err) => Err(err),
            }
        }
    }

    fn update_dpi(&mut self, dpi: u32) {
        unsafe {
            self.actual_font_size = MulDiv(16, dpi as i32, 96);
        }

        let h_font;
        unsafe {
            h_font = CreateFontW(
                self.actual_font_size,
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                0,
                w!("Segoe UI"),
            );
        }
        self.font = h_font;
    }

    fn set_event_callback(&mut self, callback: Box<dyn Fn(ReactiveEvent) + '_>) {}
    fn set_window_position(&mut self, x: i32, y: i32, width: i32, height: i32) {
        let actual_y = {
            let dc = DeviceContext::get_dc(self.get_hwnd());

            dc.select_object(HGDIOBJ::from(self.font));
            let mut rect = RECT {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            dc.calc_text_height(&self.text, &mut rect)
        };
        self.set_window_pos(x, y, width, actual_y);
        self.actual_y = actual_y;
    }

    fn get_dimensions(&self) -> (i32, i32) {
        (self.actual_x, self.actual_y)
    }
}

impl Text {
    pub fn new(inst: HINSTANCE, text: &str) -> Self {
        let mut text = Text {
            base: BaseWin::default(),
            inst,
            hwnd: HWND::default(),
            text: text.to_owned(),
            actual_x: 0,
            actual_y: 0,
            font: HFONT::default(),
            actual_font_size: 16,
        };

        let _ = text.create_component();
        text
    }

    fn create_component(&mut self) -> Result<()> {
        let create_args = WinCreateArgs {
            instance: self.inst.into(),
            class_name: w!("text"),
            ..WinCreateArgs::default()
        };
        self.create_comp(&create_args)
    }
}

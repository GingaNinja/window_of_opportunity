use crate::components::Div;
use crate::win::{Button, Component};
use crate::win_create_args::WinCreateArgs;
use crate::{default_win_impl, BaseWin, Event, EventHandled, Win};

use crate::module::WPModule;
use dioxus_core::{ElementId, VirtualDom, WriteMutations};
use windows::Win32::UI::WindowsAndMessaging::HACCEL;
use windows::{
    core::*,
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
};

pub struct WPApp {
    module: WPModule,
    pub main_win: ReactiveWindow,
    exit_code: WPARAM,
    accel: Option<HACCEL>,
    create_args: Option<WinCreateArgs>,
}

impl WPApp {
    pub fn new() -> Self {
        let module = WPModule::new();
        let main_win = ReactiveWindow::new(module.hinst);

        // self.main_win = Some(main_win);
        WPApp {
            module,
            main_win,
            exit_code: WPARAM(0),
            accel: None,
            create_args: None,
        }
    }

    pub fn new_with_config(create_args: WinCreateArgs) -> Self {
        let mut app = Self::new();
        app.create_args = Some(create_args);
        app
    }

    pub fn main_win_created(&mut self) -> () {}

    pub fn init(&mut self, title: PCWSTR) -> Result<()> {
        // self.main_win
        //     .set_created_callback(Box::new(|win| vdom.rebuild(self)));
        match &self.create_args {
            None => {
                self.main_win.create_window(title)?;
            }
            Some(create_args) => {
                self.main_win.create_window_with_args(title, create_args)?;
            }
        }

        match self.load_accelerators() {
            Ok(accel) => self.accel = Some(accel),
            Err(_err) => println!("couldn't load accelerator"),
        }

        Ok(())
    }

    pub fn load_accelerators(&self) -> Result<HACCEL> {
        unsafe { LoadAcceleratorsW(self.get_hinstance(), w!("AppAccel")) }
    }

    pub fn get_hinstance(&self) -> HINSTANCE {
        self.module.get_hinstance()
    }

    pub fn exit_code(&self) -> WPARAM {
        self.exit_code
    }

    pub fn get_message(msg: &mut MSG) -> bool {
        unsafe { GetMessageW(msg, None, 0, 0).into() }
    }

    pub fn peek_message(msg: &mut MSG) -> bool {
        unsafe { PeekMessageW(msg, None, 0, 0, PM_REMOVE).into() }
    }

    pub fn translate_accelerator(&self, accel: HACCEL, msg: MSG) -> bool {
        let msg = &msg as *const _;
        unsafe { TranslateAcceleratorW(self.main_win.get_hwnd(), accel, msg) > 0 }
    }

    pub fn translate_message(msg: &MSG) -> bool {
        unsafe { TranslateMessage(msg).as_bool() }
    }

    pub fn dispatch_message(msg: &MSG) {
        unsafe {
            DispatchMessageW(msg);
        }
    }

    pub fn run<F>(&mut self, mut f: F) -> ()
    where
        F: FnMut(&mut Self) -> (),
    {
        self.main_win.show();
        self.main_win.update();

        let mut msg = MSG::default();
        let mut peek = true;

        while peek || Self::get_message(&mut msg) {
            if peek {
                // Use PeekMessage instead of GetMessage
                if Self::peek_message(&mut msg) {
                    peek = self.main_win.do_idle();
                    continue;
                }
                if msg.message == WM_QUIT {
                    break;
                }
            }
            let accel_message = match self.accel {
                None => false,
                Some(accel) => self.translate_accelerator(accel, msg),
            };
            if !accel_message {
                Self::translate_message(&msg);
                Self::dispatch_message(&msg);
            }
            f(self);
        }

        self.exit_code = msg.wParam
    }
}

pub struct ReactiveWindow
// where
//     F: FnMut(Self) -> (),
{
    base: BaseWin,
    inst: HINSTANCE,
    child: Option<Component>,
    created: bool,
    created_callback: Option<Box<dyn FnMut(&mut ReactiveWindow) -> ()>>,
}

impl ReactiveWindow {
    pub fn set_created_callback(
        &mut self,
        callback: impl FnMut(&mut ReactiveWindow) -> () + 'static,
    ) -> () {
        self.created_callback = Some(Box::new(callback));
    }
}

impl Win for ReactiveWindow {
    default_win_impl!();

    fn new(inst: HINSTANCE) -> Self {
        Self {
            base: BaseWin::default(),
            inst: inst,
            child: None,
            created: false,
            created_callback: None,
        }
    }
    fn create_window_with_args(
        &mut self,
        title: PCWSTR,
        create_args: &WinCreateArgs,
    ) -> Result<HWND> {
        self.create_win(title, create_args, self.inst)
    }

    fn set_child(&mut self, mut child: Component) {
        if self.created {
            child.create_element(self.get_hwnd(), self.inst).unwrap();
        }
        self.child = Some(child);
    }

    fn create_window(&mut self, title: PCWSTR) -> Result<HWND> {
        let create_args = WinCreateArgs {
            instance: self.inst.into(),
            ..WinCreateArgs::default_win_main()
        };
        self.create_win(title, &create_args, self.inst)
    }

    fn on_create(&mut self, _event: &Event) -> EventHandled {
        self.created = true;
        let child = self.child.take();
        if let Some(mut child) = child {
            child.create_element(self.get_hwnd(), self.inst);
            self.child = Some(child);
        };
        println!("created window...");
        let callback = self.created_callback.take();
        if let Some(mut callback) = callback {
            println!("calling callback...");
            callback(self);

            self.created_callback = Some(callback);
        } else {
            self.created_callback = None;
        }
        EventHandled::Handled(LRESULT(0))
    }
}

impl WriteMutations for ReactiveWindow {
    fn append_children(&mut self, id: ElementId, m: usize) {
        println!("append_children... id: {id:?}, m: {m}");
        let button = Button::new(1, w!("testing123"))
            .with_height(30)
            .with_width(100);
        let div = Div::new(self.inst);
        self.set_child(Component::Container(Box::new(div)));
        //self.set_child(Component::Element(Box::new(button)));
        self.set_window_text(w!("we just set the text ad-hoc"));
    }

    fn assign_node_id(&mut self, _path: &'static [u8], _id: ElementId) {
        todo!()
    }

    fn create_placeholder(&mut self, _id: ElementId) {
        todo!()
    }

    fn create_text_node(&mut self, _value: &str, _id: ElementId) {
        todo!()
    }

    fn load_template(&mut self, template: dioxus_core::Template, _index: usize, _id: ElementId) {
        println!("load template: {template:?}")
    }

    fn replace_node_with(&mut self, _id: ElementId, _m: usize) {
        todo!()
    }

    fn replace_placeholder_with_nodes(&mut self, _path: &'static [u8], _m: usize) {
        todo!()
    }

    fn insert_nodes_after(&mut self, _id: ElementId, _m: usize) {
        todo!()
    }

    fn insert_nodes_before(&mut self, _id: ElementId, _m: usize) {
        todo!()
    }

    fn set_attribute(
        &mut self,
        _name: &'static str,
        _ns: Option<&'static str>,
        _value: &dioxus_core::AttributeValue,
        _id: ElementId,
    ) {
        todo!()
    }

    fn set_node_text(&mut self, _value: &str, _id: ElementId) {
        todo!()
    }

    fn create_event_listener(&mut self, _name: &'static str, _id: ElementId) {
        todo!()
    }

    fn remove_event_listener(&mut self, _name: &'static str, _id: ElementId) {
        todo!()
    }

    fn remove_node(&mut self, _id: ElementId) {
        todo!()
    }

    fn push_root(&mut self, _id: ElementId) {
        todo!()
    }
}

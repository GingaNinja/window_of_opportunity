use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

use crate::components::{Div, Text};
use crate::win::{Button, Component, Observer};
use crate::win_create_args::WinCreateArgs;
use crate::{default_win_impl, BaseWin, Event, EventHandled, Win};

use crate::module::WPModule;
use dioxus_core::{ElementId, TemplateNode, WriteMutations};
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

/// The state of the Dioxus integration with the win32 windows
#[derive(Debug)]
pub struct DioxusState {
    /// Store of templates keyed by unique name
    //templates: FxHashMap<Template, Vec<NodeId>>,
    /// Stack machine state for applying dioxus mutations
    stack: Vec<Rc<dyn Component>>,
    /// Mapping from vdom ElementId -> treepath
    treepath_mapping: Vec<Vec<i32>>,
    counter: usize,
}

impl DioxusState {
    fn new(mount_point: Rc<dyn Component>) -> Self {
        Self {
            stack: vec![mount_point],
            treepath_mapping: vec![vec![0]],
            counter: 0,
        }
    }

    /// Convert an ElementId to a treepath
    pub fn element_to_treepath(&self, element_id: ElementId) -> &[i32] {
        // self.try_element_to_treepath(element_id).unwrap()
        self.treepath_mapping.get(element_id.0).unwrap()
    }

    // Attempt to convert an ElementId to a treepath. This will return None if the ElementId is not in the RealDom.
    // pub fn try_element_to_treepath(&self, element_id: ElementId) -> Option<Vec<i32>> {
    //     self.treepath_mapping.get(element_id.0).unwrap()
    //     //.copied().flatten()
    // }
}

pub struct ReactiveWindow {
    base: BaseWin,
    inst: HINSTANCE,
    child: Option<Rc<dyn Component>>,
    created: bool,
    event_callback: Option<Box<dyn FnMut(&mut ReactiveWindow, ReactiveEvent)>>,
}

#[derive(Clone, Copy)]
pub enum ReactiveEvent {
    Created,
    Command,
}

#[derive(Debug)]
struct WindowPlaceholder {}
impl Component for WindowPlaceholder {
    fn create_element(
        &mut self,
        _parent: HWND,
        _instance: HINSTANCE,
        _parent_rect: &RECT,
    ) -> Result<HWND> {
        panic!("this is just a placeholder, shouldn't be trying to display it...");
    }

    fn set_window_position(&mut self, _x: i32, _y: i32, _width: i32, _height: i32) {
        todo!()
    }

    fn get_dimensions(&self) -> (i32, i32) {
        todo!()
    }
}

impl Win for ReactiveWindow {
    default_win_impl!();

    fn on_resize(&mut self, x: i32, y: i32) -> EventHandled {
        if let Some(child) = &mut self.child {
            child.set_window_position(0, 0, x, y);
        }
        EventHandled::Handled(LRESULT(0))
    }

    fn new(inst: HINSTANCE) -> Self {
        Self {
            base: BaseWin::default(),
            inst,
            child: None,
            created: false,
            event_callback: None,
        }
    }
    fn create_window_with_args(
        &mut self,
        title: PCWSTR,
        create_args: &WinCreateArgs,
    ) -> Result<HWND> {
        self.create_win(title, create_args, self.inst)
    }

    fn set_child(&mut self, mut _child: Box<dyn Component>) {}

    fn create_window(&mut self, title: PCWSTR) -> Result<HWND> {
        let create_args = WinCreateArgs {
            instance: self.inst.into(),
            ..WinCreateArgs::default_win_main()
        };
        self.create_win(title, &create_args, self.inst)
    }

    fn update_child_dpi(&mut self, dpi: u32) {
        let child = self.child.as_mut();
        if let Some(child) = child {
            child.update_dpi(dpi);
        }
    }

    fn on_create(&mut self, _event: &Event) -> EventHandled {
        self.created = true;
        let child = self.child.take();
        if let Some(mut child) = child {
            let rect = RECT {
                left: 0,
                top: 0,
                right: self.get_base().x,
                bottom: self.get_base().y,
            };

            child
                .create_element(self.get_hwnd(), self.inst, &rect)
                .unwrap();
            self.child = Some(child);
        };
        println!("created window (some new text)...");
        let callback = self.event_callback.take();
        if let Some(mut callback) = callback {
            println!("calling callback...");
            callback(self, ReactiveEvent::Created);

            self.event_callback = Some(callback);
        } else {
            self.event_callback = None;
        }
        EventHandled::Handled(LRESULT(0))
    }

    fn on_command(&mut self, _event: &crate::CommandEvent) -> EventHandled {
        let callback = self.event_callback.take();
        println!("in the 'on_command' handler");
        if let Some(mut callback) = callback {
            callback(self, ReactiveEvent::Command);
            self.event_callback = Some(callback);
        }
        EventHandled::Handled(LRESULT(0))
    }
}

impl Observer for ReactiveWindow {
    fn notify(&self, event: ReactiveEvent) {
        println!("event passed to windwo");
    }
}

impl ReactiveWindow {
    fn set_child(self: Rc<Self>, mut child: Rc<dyn Component>) {
        let self_ref = self.clone();
        let comp_self = self_ref as Rc<dyn Observer>;

        child.register(Rc::downgrade(&comp_self));
        // child.set_event_callback(Box::new(|event| {
        //     println!("event passed to window");
        //     // if let Some(callback) = callback {}
        // }));
        if self.created {
            let rect = RECT {
                left: 0,
                top: 0,
                right: self.get_base().x,
                bottom: self.get_base().y,
            };
            child
                .create_element(self.get_hwnd(), self.inst, &rect)
                .unwrap();
        }
        self.child = Some(child);
    }
}
pub struct ReactiveWindowWrapper {
    window: Rc<ReactiveWindow>,
    state: DioxusState,
}

impl ReactiveWindowWrapper {
    fn new(window: Rc<ReactiveWindow>) -> Self {
        Self {
            window,
            state: DioxusState::new(Rc::new(WindowPlaceholder {})),
        }
    }
    fn create_template_node(&mut self, node: &TemplateNode) -> Rc<dyn Component> {
        match node {
            dioxus_core::TemplateNode::Text { text } => {
                let text = Text::new(self.window.inst, text);
                Rc::new(text)
            }
            dioxus_core::TemplateNode::Element {
                tag,
                namespace: _,
                attrs,
                children,
            } if *tag == "div" => {
                let mut div = Div::new(self.window.inst);
                if attrs.len() > 0 {
                    div.set_bk_colour(0x00E2E2FE);
                }
                let children: Vec<_> = children
                    .iter()
                    .map(|child| self.create_template_node(child))
                    .collect();
                for child in children {
                    div.add_child(child);
                }
                Box::new(div)
            }
            dioxus_core::TemplateNode::Element {
                tag,
                namespace: _,
                attrs: _,
                children,
            } if *tag == "button" => {
                let text = match children.first().unwrap() {
                    dioxus_core::TemplateNode::Text { text } => text,
                    _ => "",
                };
                self.state.counter += 1;
                let button = Button::new(self.state.counter, text.to_owned());
                Rc::new(button)
            }
            dioxus_core::TemplateNode::Dynamic { .. } => Box::new(WindowPlaceholder {}),
            unknown => {
                println!("unrecognised node type: {unknown:?}");
                Rc::new(WindowPlaceholder {})
            }
        }
    }
}

impl WriteMutations for ReactiveWindowWrapper {
    fn append_children(&mut self, id: ElementId, m: usize) {
        println!("append_children... id: {id:?}, m: {m}");

        let children = self.state.stack.split_off(self.state.stack.len() - m);
        let parent = self.state.element_to_treepath(id);
        let _self_is_parent = if parent.len() == 1 { true } else { false };
        let window = self.window.clone();
        for child in children {
            window.clone().set_child(child);
        }
        window.set_window_text(w!("we just set the text ad-hoc"));
    }

    fn assign_node_id(&mut self, path: &'static [u8], id: ElementId) {
        println!("assign_node_id: {path:?}, id: {id:?}");
    }

    fn create_placeholder(&mut self, _id: ElementId) {
        todo!()
    }

    fn create_text_node<'a>(&mut self, value: &'a str, id: ElementId) {
        println!("create_text_node: {value}, {id:?}");
        let text = Text::new(self.window.inst, value);
        self.state.stack.push(Rc::new(text));
    }

    fn load_template(&mut self, template: dioxus_core::Template, index: usize, id: ElementId) {
        println!("load template: {template:?} - index: {index}, id: {id:?}");

        let new_node = self.create_template_node(template.roots.first().unwrap());
        self.state.stack.push(new_node);
    }

    fn replace_node_with(&mut self, _id: ElementId, _m: usize) {
        todo!()
    }

    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize) {
        println!("replace_placeholder_with_nodes(path: {path:?}, m: {m:?})");
        let new_nodes = self.state.stack.split_off(self.state.stack.len() - m);

        let stack_len = self.state.stack.len();
        let mut current_node = self.state.stack[stack_len - 1]; // .get(stack_len - 1).unwrap();
        let (last, path) = path.split_last().unwrap();
        for i in path {
            current_node = current_node.clone().get_child(*i as usize);
        }
        current_node.swap_node_with_nodes(*last as usize, new_nodes);
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

    fn create_event_listener(&mut self, name: &'static str, id: ElementId) {
        println!("create_event_listener(name: {name:?}, id: {id:?})");
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

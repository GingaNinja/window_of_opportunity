// ---------------------------------------------------------------------------
// The win32 driver. The cacao side models this as an AppDelegate + Dispatcher
// + WindowProxy receiving AppKit callbacks; here one WndProc + a
// GWLP_USERDATA pointer plays all three roles (the mapping lives in
// docs/win32-port-notes.md):
//
//   did_finish_launching  → the sequence in `run` after CreateWindowExW
//   Dispatcher (Message<M>) → WM_APP_EVENT / WM_APP_MSG + `dispatch`
//   WindowProxy.did_resize → WM_SIZE
//
// Step-2 scope: Window/Div/Button/Text. Input/Image/List land next (their
// seams are marked in `mount_element` and the WndProc below).
// ---------------------------------------------------------------------------

use std::{
    any::Any,
    cell::{Cell, RefCell},
    mem,
    rc::Rc,
    sync::atomic::{AtomicIsize, Ordering},
};

use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::{
            CreateSolidBrush, DeleteObject, FillRect, HBRUSH, HDC, InvalidateRect, SetBkMode,
            TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::*,
};

use crate::{
    component::Component,
    element::{Element, ElementType, PropType, button_label, window_spec},
    reconcile,
    state::{Ctx, Handler, Handlers, State},
};

use super::{
    stack::{self, Rect},
    util::{get_utf16_vec, load_cursor, load_icon},
    widgets::{self, Widget},
};

/// Shared with every backend — the semantics live on `state::Message`.
pub use crate::state::Message;

/// Widget events ride the queue as `WM_APP + 1` (wparam = dispatch id);
/// app messages as `WM_APP + 2` (lparam = `Box<dyn Any + Send>`).
const WM_APP_EVENT: u32 = WM_APP + 1;
const WM_APP_MSG: u32 = WM_APP + 2;
/// "what brush do you paint with?" — asked by undecorated divs that
/// inherit their parent's backdrop (the transparent-NSView behavior)
const WM_GET_BG_BRUSH: u32 = WM_APP + 3;

const CLASS_NAME: PCWSTR = w!("wo_mainwin");
const DIV_CLASS: PCWSTR = w!("wo_div");

/// Control id for "this control has no handler" — also win32's NULL HMENU
/// (id 0 = "no identifier"). Dispatch ids start at 1 (see
/// `Handlers::default`), so the two spaces can't collide.
const NO_HANDLER_ID: usize = 0;

/// The main window, so `dispatch` works from any thread (PostMessage is
/// thread-safe and lands on the message-loop thread).
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

pub struct Application {}

impl Application {
    /// Identical shape to the macOS `run`: the platform delegate machinery
    /// hides behind it. There: `App::launch` + AppDelegate. Here: the window
    /// class + message loop below.
    pub fn run<M: Send + Sync + 'static>(
        &self,
        root: Box<dyn Component>,
        on_app_message: impl Fn(&State, M) + 'static,
    ) {
        unsafe {
            // (GetModuleHandleW hands back an HMODULE; the window APIs
            // want the HINSTANCE-shaped alias of the same handle)
            let hinst = HINSTANCE(GetModuleHandleW(None).expect("GetModuleHandleW").0);

            // Render once before the window exists — same reason as macOS:
            // resizability is set at creation, and a specified size gives the
            // window its initial dimensions.
            let scratch = State::default();
            let spec = window_spec(&root.render(&Ctx { state: &scratch }, vec![]));

            // style + initial CLIENT size → window size (the win32 answer to
            // the macOS titlebar math: AdjustWindowRectEx adds the chrome,
            // and the client rect needs no offset at all)
            let style = WS_OVERLAPPEDWINDOW;
            let ex_style = WS_EX_APPWINDOW;
            let client_w = spec.width.unwrap_or(1024.) as i32;
            let client_h = spec.height.unwrap_or(768.) as i32;
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: client_w,
                bottom: client_h,
            };
            let _ = AdjustWindowRectEx(&mut rect, style, false, ex_style);
            let win_w = rect.right - rect.left;
            let win_h = rect.bottom - rect.top;

            register_class(hinst);
            register_div_class(hinst);

            // The app-message adapter: downcasts the erased payload back to
            // M. This is what keeps AppState non-generic (the cacao side
            // achieves the same by naming the types only in `dispatch_main`).
            let on_app_message: Box<dyn Fn(&State, Box<dyn Any + Send>)> =
                Box::new(move |state, message| {
                    let message = message.downcast::<M>().expect("app message type");
                    on_app_message(state, *message);
                });

            let state = Rc::new(RefCell::new(AppState {
                root,
                state: State::default(),
                handlers: Handlers::new(),
                root_widget: None,
                last_tree: None,
                hwnd: HWND(std::ptr::null_mut()), // filled in right after creation
                on_app_message,
                last_requested_size: Cell::new(None),
            }));

            // hand the Rc to the window via lpParam — WM_NCCREATE parks it
            // in GWLP_USERDATA and the WndProc borrows through it
            let title_wide = get_utf16_vec(&spec.title);
            let state_ptr = Box::into_raw(Box::new(state.clone()));
            let hwnd = CreateWindowExW(
                ex_style,
                CLASS_NAME,
                PCWSTR(title_wide.as_ptr()),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                win_w,
                win_h,
                None,
                None,
                Some(hinst),
                Some(state_ptr as *const _ as _),
            )
            .expect("CreateWindowExW");
            state.borrow_mut().hwnd = hwnd;
            MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

            // first render + show — the win32 `did_finish_launching`
            state.borrow_mut().render();
            let _ = ShowWindow(hwnd, SW_SHOW);

            // the message loop (the old WPApp::run's body, simplified: a
            // plain GetMessage pump — the reactive model repaints on
            // messages only, no idle painting)
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

/// Puts an app message onto the main queue. Call from any thread — the
/// app-side half of the Send boundary.
pub fn dispatch<M: Send + Sync + 'static>(message: M) {
    // double-box: the inner box is fat (dyn Any), the outer is thin and
    // survives the round trip through LPARAM
    let payload: Box<dyn Any + Send> = Box::new(message);
    let ptr = Box::into_raw(Box::new(payload));
    unsafe {
        let _ = PostMessageW(
            Some(HWND(MAIN_HWND.load(Ordering::SeqCst) as *mut _)),
            WM_APP_MSG,
            WPARAM(0),
            LPARAM(ptr as *mut () as isize),
        );
    }
}

fn register_class(hinst: HINSTANCE) {
    unsafe {
        let wc = WNDCLASSEXW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hCursor: load_cursor(None, IDC_ARROW).unwrap(),
            hIcon: load_icon(hinst, IDI_APPLICATION).unwrap_or_default(),
            // the backdrop behind the root — the macOS content view's gray
            // (rgb 151,143,143) so undecorated content blends the same way
            // on both platforms. Leaked by design: the class owns it for
            // the process's life.
            hbrBackground: CreateSolidBrush(COLORREF(0x008F_8F97)),
            lpszClassName: CLASS_NAME,
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        debug_assert!(atom != 0);
    }
}

/// The Div class: no class brush (divs paint themselves), the brush rides
/// in each window's GWLP_USERDATA (set from lpParam at creation).
fn register_div_class(hinst: HINSTANCE) {
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
                if code == BN_CLICKED {
                    // button clicks bubble up to the window
                    if let Ok(parent_hwnd) = GetParent(hwnd) {
                        return SendMessageW(parent_hwnd, WM_COMMAND, Some(wparam), Some(lparam));
                    }
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

/// Windows color mapping — the Apple system (light) colors the cacao
/// `color()` uses, as COLORREF (0x00BBGGRR), so the same ui! code looks
/// right on both platforms.
fn color_ref(name: &str) -> COLORREF {
    match name {
        "blue" => COLORREF(0x00FF_7A00),  // rgb(0, 122, 255)
        "red" => COLORREF(0x0030_3BFF),   // rgb(255, 59, 48)
        "green" => COLORREF(0x0058_D130), // rgb(48, 209, 88)
        "gray" => COLORREF(0x0093_8E8E),  // rgb(142, 142, 147)
        _ => COLORREF(0x005E_84A2),       // rgb(162, 132, 94) — SystemBrown
    }
}

fn color_brush(name: &str) -> HBRUSH {
    unsafe { CreateSolidBrush(color_ref(name)) }
}

pub(crate) struct AppState {
    root: Box<dyn Component>,
    pub state: State,
    pub handlers: Handlers,
    root_widget: Option<Widget>,
    /// the previous render's expanded element tree — the diff target
    last_tree: Option<Box<Element>>,
    hwnd: HWND,
    // NOTE: unlike the cacao side there's no `dispatch_event` closure here:
    // widget events arrive as WM_COMMAND already on the loop thread, so the
    // queue hop is redundant. Cross-thread talk goes through `dispatch`.
    /// app messages arrive erased (so AppState stays non-generic); the
    /// adapter built in `run` downcasts them back to M
    on_app_message: Box<dyn Fn(&State, Box<dyn Any + Send>)>,
    /// the client size the last render requested — the controlled-size rule
    /// (writes happen when the request changes, not when actual drifts)
    last_requested_size: Cell<Option<(i32, i32)>>,
}

impl AppState {
    /// The full render pipeline, same shape as the macOS one: expand →
    /// reconcile → fit the window → arrange.
    pub fn render(&mut self) {
        let tree = self.root.render(&Ctx { state: &self.state }, vec![]);
        // expand component nodes so patch/mount only ever see primitives
        let tree = reconcile::expand(&self.state, &tree);

        #[cfg(feature = "debug_dump")]
        println!("{tree:#?}");

        let spec = window_spec(&tree);
        self.handlers
            .set_resize_handler(tree.handlers.get("on_resize").cloned());

        match (self.last_tree.take(), self.root_widget.take()) {
            (Some(old_tree), Some(mut root_widget)) => {
                self.patch(&mut root_widget, &old_tree, &tree);
                self.root_widget = Some(root_widget);
            }
            (_, root_widget) => {
                // first render: mount fresh
                let root_widget =
                    root_widget.unwrap_or_else(|| self.mount_element(self.hwnd, &tree));
                self.root_widget = Some(root_widget);
            }
        }
        let title = get_utf16_vec(&spec.title);

        unsafe {
            let _ = SetWindowTextW(self.hwnd, PCWSTR(title.as_ptr()));
        }

        // Sizing: explicit props win; a missing axis hugs the content.
        // Controlled-component rule (same as macOS): only write the window
        // size when the REQUEST changes — a user resize in between is the
        // user's business.
        let (natural_w, natural_h) = match self.root_widget.as_ref() {
            Some(widget) => stack::natural(&tree, widget),
            None => (1, 1),
        };
        let client_w = spec.width.map(|w| w as i32).unwrap_or(natural_w);
        let client_h = spec.height.map(|h| h as i32).unwrap_or(natural_h);
        if self.last_requested_size.replace(Some((client_w, client_h)))
            != Some((client_w, client_h))
        {
            unsafe {
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: client_w,
                    bottom: client_h,
                };
                let _ = AdjustWindowRectEx(&mut rect, WS_OVERLAPPEDWINDOW, false, WS_EX_APPWINDOW);
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    0,
                    0,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }

        // arrange the tree in the client rect we actually have (this can
        // fire WM_SIZE synchronously — the WndProc skips it via try_borrow)
        if let Some(widget) = self.root_widget.as_mut() {
            let mut rect = RECT::default();
            unsafe {
                let _ = GetClientRect(self.hwnd, &mut rect);
            }
            stack::arrange(
                &tree,
                widget,
                Rect {
                    x: 0,
                    y: 0,
                    w: rect.right - rect.left,
                    h: rect.bottom - rect.top,
                },
            );
        }

        self.last_tree = Some(tree);
    }

    fn mount_element(&self, parent: HWND, el: &Element) -> Widget {
        match &el.element_type {
            // the root's container hwnd IS the window itself
            ElementType::Window => Widget::Container {
                hwnd: parent,
                background: None,
                children: el
                    .children
                    .iter()
                    .map(|child| self.mount_element(parent, child))
                    .collect(),
            },
            ElementType::Div => {
                // a Div is a real child window — the win32 analogue of the
                // cacao view: it paints its own background and its
                // children's controls live inside it
                let background = el.props.get_string(PropType::Background).map(color_brush);
                let hwnd = self.create_div(parent, background);
                Widget::Container {
                    hwnd,
                    background,
                    children: el
                        .children
                        .iter()
                        .map(|child| self.mount_element(hwnd, child))
                        .collect(),
                }
            }
            ElementType::Text(text) => Widget::Label {
                // SS_LEFT is literally the empty style bits (left-aligned is
                // a static's default) — pass 0 and skip the SystemServices
                // feature just for a zero constant
                hwnd: self.create_control(w!("static"), text, 0, parent, NO_HANDLER_ID),
            },
            ElementType::Button => {
                // The dispatch id IS the control id — WM_COMMAND carries it
                // back. (16-bit payload: 65k handlers per window is plenty
                // for now; a lookup table is the fix if it ever isn't.)
                let handler_id = match el.handlers.get("on_click") {
                    Some(Handler::Simple(event)) => Some(self.handlers.register(event)),
                    _ => None,
                };
                let id = handler_id.unwrap_or(NO_HANDLER_ID);
                debug_assert!(id <= 0xffff, "dispatch id must fit WM_COMMAND's 16 bits");
                Widget::Button {
                    hwnd: self.create_control(
                        w!("button"),
                        &button_label(el),
                        BS_PUSHBUTTON as u32,
                        parent,
                        id,
                    ),
                    handler_id,
                }
            }
            ElementType::Input | ElementType::Image | ElementType::List => {
                todo!("win32 step 2: Input/Image/List land next")
            }
            ElementType::Component(_) => {
                unreachable!("component elements are expanded before mounting")
            }
        }
    }

    /// A Div's child window — its own background brush rides along as
    /// lpParam (null = inherit the parent's).
    fn create_div(&self, parent: HWND, background: Option<HBRUSH>) -> HWND {
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
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

    /// Creates a child control. `style` is the class-specific style bits
    /// (the windows crate types these inconsistently — i32, STATIC_STYLES —
    /// so they arrive raw and join WS_CHILD | WS_VISIBLE here).
    fn create_control(
        &self,
        class: PCWSTR,
        text: &str,
        style: u32,
        parent: HWND,
        id: usize,
    ) -> HWND {
        let text_wide = get_utf16_vec(text);
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                PCWSTR(text_wide.as_ptr()),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | style),
                0,
                0,
                10,
                10, // arrange() positions it before anything is visible
                Some(parent),
                Some(HMENU(id as isize as *mut _)), // for a child, hmenu IS its control id
                Some(HINSTANCE(GetModuleHandleW(None).unwrap().0)),
                None,
            )
            .expect("CreateWindowExW control")
        }
    }

    /// Reconciliation, same shape as the macOS `patch`: reuse widgets whose
    /// elements line up, refresh props and handlers in place.
    fn patch(&self, widget: &mut Widget, old_el: &Element, new_el: &Element) {
        match (widget, &new_el.element_type) {
            (widget @ Widget::Container { .. }, ElementType::Window | ElementType::Div) => {
                self.patch_container(widget, old_el, new_el);
            }
            (Widget::Button { hwnd, handler_id }, ElementType::Button) => {
                // reconcile the title
                if button_label(old_el) != button_label(new_el) {
                    let title = get_utf16_vec(&button_label(new_el));
                    unsafe {
                        let _ = SetWindowTextW(*hwnd, PCWSTR(title.as_ptr()));
                    }
                }
                // refresh the handler under the same dispatch id — the
                // control keeps firing this id while the handler stays current
                if let Some(Handler::Simple(event)) = new_el.handlers.get("on_click") {
                    match handler_id {
                        Some(id) => self.handlers.refresh(*id, event),
                        None => {
                            let id = self.handlers.register(event);
                            debug_assert!(id <= 0xffff);
                            unsafe {
                                let _ = SetWindowLongPtrW(*hwnd, GWLP_ID, id as isize);
                            }
                            *handler_id = Some(id);
                        }
                    }
                }
            }
            (Widget::Label { hwnd }, ElementType::Text(text)) => {
                // display-only: no cursor to protect, just refresh
                let title = get_utf16_vec(text);
                unsafe {
                    let _ = SetWindowTextW(*hwnd, PCWSTR(title.as_ptr()));
                }
            }
            // the caller only patches compatible pairs
            _ => unreachable!("patch called on an incompatible widget/element pair"),
        }
    }

    fn patch_container(&self, widget: &mut Widget, old_el: &Element, new_el: &Element) {
        let Widget::Container {
            hwnd,
            background,
            children,
        } = widget
        else {
            unreachable!("patch_container called on a non-container")
        };

        // Reconcile the background prop (visual only) — the cacao twin of
        // this block sets the NSView's color. The root window is skipped:
        // its backdrop is the class brush, and its GWLP_USERDATA holds the
        // app, not a brush.
        if *hwnd != self.hwnd
            && old_el.props.get_string(PropType::Background)
                != new_el.props.get_string(PropType::Background)
        {
            let new_brush = new_el
                .props
                .get_string(PropType::Background)
                .map(color_brush);
            if let Some(old) = std::mem::replace(background, new_brush) {
                unsafe {
                    let _ = DeleteObject(old.into());
                }
            }
            unsafe {
                SetWindowLongPtrW(
                    *hwnd,
                    GWLP_USERDATA,
                    background.map_or(0, |brush| brush.0 as isize),
                );
                let _ = InvalidateRect(Some(*hwnd), None, true);
            }
        }

        // the shared children-diff skeleton with this platform's ops as
        // closures (no relayout flag needed here — arrange runs wholesale
        // after every render)
        reconcile::reconcile_children(
            children,
            &old_el.children,
            &new_el.children,
            widgets::compatible,
            |child_widget, old_child, new_child| self.patch(child_widget, old_child, new_child),
            |child_el| self.mount_element(self.hwnd, child_el),
        );
    }

    /// App messages arrive erased (so AppState stays non-generic): the
    /// adapter built in `run` downcasts them back to M.
    pub fn deliver_app_message(&mut self, message: Box<dyn Any + Send>) {
        (self.on_app_message)(&self.state, message);
    }
}

/// The React loop's win32 shape: fire the handler for a dispatch id, then
/// re-render. `try_borrow` because messages can arrive mid-render (our own
/// SetWindowPos fires WM_SIZE synchronously) — a render in flight wins.
fn fire_and_render(app: &RefCell<AppState>, id: usize) {
    if let Ok(mut app) = app.try_borrow_mut() {
        if app.handlers.fire(id, &app.state) {
            app.render();
        }
    }
}

/// The WndProc is the delegate: `did_finish_launching` happened in `run`,
/// and everything below is Dispatcher + WindowProxy + (later) InputDelegate.
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Rc<RefCell<AppState>>;
        if ptr.is_null() {
            if msg == WM_NCCREATE {
                // lpParam is the Rc<RefCell<AppState>> — park it here for every
                // later message (the GWLP_USERDATA pattern from the old code,
                // now leading to the app instead of a `Win`)
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            }
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let app = &*ptr;

        match msg {
            WM_COMMAND => {
                // child controls report here: LOWORD(wparam) is the control id
                // (== dispatch id), HIWORD the notification code
                let id = (wparam.0 & 0xffff) as usize;
                let code = ((wparam.0 >> 16) & 0xffff) as u32;
                if code == BN_CLICKED {
                    fire_and_render(app, id);
                    return LRESULT(0);
                }
            }
            WM_APP_EVENT => {
                fire_and_render(app, wparam.0 as usize);
                return LRESULT(0);
            }
            WM_APP_MSG => {
                // the app-side half of the Send boundary (see `dispatch`)
                let message = *Box::from_raw(lparam.0 as *mut Box<dyn Any + Send>);
                if let Ok(mut app) = app.try_borrow_mut() {
                    app.deliver_app_message(message);
                    app.render();
                }
                return LRESULT(0);
            }
            WM_SIZE => {
                // the WindowProxy.did_resize role: user resizes flow through
                // component logic (on_resize), then the tree follows. lparam
                // carries the new client size (loword = width, hiword = height)
                if let Ok(mut app) = app.try_borrow_mut() {
                    let (w, h) = (
                        (lparam.0 & 0xffff) as f64,
                        ((lparam.0 >> 16) & 0xffff) as f64,
                    );
                    if let Some(Handler::Resize(f)) = app.handlers.resize_handler() {
                        f(&app.state, w, h);
                    }
                    app.render();
                }
                return LRESULT(0);
            }
            WM_DESTROY => PostQuitMessage(0),
            WM_GET_BG_BRUSH => {
                // children of the root ask for its backdrop — the class brush
                return LRESULT(GetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND) as isize);
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr as *mut Rc<RefCell<AppState>>));
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

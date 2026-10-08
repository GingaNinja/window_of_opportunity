// ---------------------------------------------------------------------------
// The win32 driver. The cacao side models this as an AppDelegate + Dispatcher
// + WindowProxy receiving AppKit callbacks; here one WndProc + a
// GWLP_USERDATA pointer plays all three roles (the mapping lives in
// docs/win32-port-notes.md):
//
//   did_finish_launching  → the sequence in `run` after CreateWindowExW
//   Dispatcher (Message<M>) → WM_APP_MSG + `dispatch`
//   WindowProxy.did_resize → WM_SIZE
//
// Step-2 scope: Window/Div/Button/Text. Input/Image/List land next (their
// seams are marked in `mount_element` and the WndProc below).
// ---------------------------------------------------------------------------

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    mem,
    rc::Rc,
    sync::atomic::{AtomicIsize, Ordering},
};

use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::{
            CreateSolidBrush, DeleteObject, GetDC, HBRUSH, HFONT, InvalidateRect, ReleaseDC,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                EM_SETCUEBANNER, ICC_LISTVIEW_CLASSES, INITCOMMONCONTROLSEX, ImageList_Destroy,
                InitCommonControlsEx, LVM_SETITEMCOUNT,
            },
            WindowsAndMessaging::*,
        },
    },
    core::*,
};

use crate::{
    component::Component,
    element::{Element, ElementType, PropType, button_label, window_spec},
    reconcile,
    state::{Ctx, Event, Handler, Handlers, State},
};

use super::{
    dc::DeviceContext,
    div::{create_div, register_div_class},
    font::{create_font, scaled_pixels},
    list::{create_list, notify, sync_lists},
    paint,
    stack::{self, Rect},
    util::{get_utf16_vec, load_cursor, load_icon},
    widgets::{self, Widget},
};

/// Shared with every backend — the semantics live on `state::Message`.
pub use crate::state::Message;

/// App messages ride the queue as `WM_APP + 2` (lparam =
/// `Box<dyn Any + Send>`). Widget events need no queue at all — WM_COMMAND
/// arrives in-loop and resolves by hwnd.
const WM_APP_MSG: u32 = WM_APP + 2;
/// "what brush do you paint with?" — asked by undecorated divs that
/// inherit their parent's backdrop (the transparent-NSView behavior)
pub const WM_GET_BG_BRUSH: u32 = WM_APP + 3;

const CLASS_NAME: PCWSTR = w!("wo_mainwin");

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
            let ex_style = WS_EX_APPWINDOW | WS_EX_CONTROLPARENT;
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
            init_common_controls();

            // The app-message adapter: downcasts the erased payload back to
            // M. This is what keeps AppState non-generic (the cacao side
            // achieves the same by naming the types only in `dispatch_main`).
            let on_app_message: AppMessageFn = Box::new(move |state, message| {
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
                fonts: RefCell::new(HashMap::new()),
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

            // the message loop — IsDialogMessageW recurses into WS_EX_CONTROLPARENT children
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                if !IsDialogMessageW(hwnd, &msg).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
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
        // the backdrop behind the root — the macOS content view's gray
        // (rgb 151,143,143) so undecorated content blends the same way
        // on both platforms. Leaked by design: the class owns it for
        // the process's life.
        let background = CreateSolidBrush(COLORREF(0x008F_8F97));
        let wc = WNDCLASSEXW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            hCursor: load_cursor(None, IDC_ARROW).unwrap(),
            hIcon: load_icon(hinst, IDI_APPLICATION).unwrap_or_default(),
            hbrBackground: background,
            lpszClassName: CLASS_NAME,
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            // a second Application in one process (the survey tests
            // boot one per test) finds the class already registered —
            // that registration is ours and fine; this call's brush is
            // spare, so free it (the class kept the first one)
            assert_eq!(
                GetLastError(),
                ERROR_CLASS_ALREADY_EXISTS,
                "RegisterClassExW"
            );
            let _ = DeleteObject(background.into());
        }
    }
}

/// Common controls (the list-view) need explicit init.
fn init_common_controls() {
    unsafe {
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES,
        });
    }
}

/// Windows color mapping — the Apple system (light) colors the cacao
/// `color()` uses, as COLORREF (0x00BBGGRR), so the same ui! code looks
/// right on both platforms.
pub(crate) fn color_ref(name: &str) -> COLORREF {
    match name {
        "blue" => COLORREF(0x00FF_7A00),  // rgb(0, 122, 255)
        "red" => COLORREF(0x0030_3BFF),   // rgb(255, 59, 48)
        "green" => COLORREF(0x0058_D130), // rgb(48, 209, 88)
        "gray" => COLORREF(0x0093_8E8E),  // rgb(142, 142, 147)
        "black" => COLORREF(0x0000_0000), // rgb(0,0,0)
        _ => COLORREF(0x005E_84A2),       // rgb(162, 132, 94) — SystemBrown
    }
}

pub(crate) fn color_brush(name: &str) -> HBRUSH {
    unsafe { CreateSolidBrush(color_ref(name)) }
}

/// erased app-message adapter: `Box<dyn Fn(&State, Box<dyn Any + Send>)>`
type AppMessageFn = Box<dyn Fn(&State, Box<dyn Any + Send>)>;

pub(crate) struct AppState {
    root: Box<dyn Component>,
    pub state: State,
    pub handlers: Handlers,
    pub root_widget: Option<Widget>,
    /// the previous render's expanded element tree — the diff target
    last_tree: Option<Box<Element>>,
    hwnd: HWND,
    // NOTE: widget events resolve by hwnd through the widget tree (handlers
    // live on their widgets) — no dispatch ids, no queue hop. The cacao
    // side needs its id registry only because objc action closures must be
    // Send (see docs/objc2-migration-notes.md). Cross-thread talk goes
    // through `dispatch`.
    /// app messages arrive erased (so AppState stays non-generic); the
    /// adapter built in `run` downcasts them back to M
    on_app_message: AppMessageFn,
    /// the client size the last render requested — the controlled-size rule
    /// (writes happen when the request changes, not when actual drifts)
    last_requested_size: Cell<Option<(i32, i32)>>,
    /// WM_SETFONT fonts, keyed by pixel height: one HFONT per size shared
    /// by every control that asks (equal sizes share a handle — a control
    /// can only wear ONE font, but each control picks its own). Controls
    /// only BORROW these; freed at teardown (see Drop below).
    ///
    /// Entries are never removed while the app runs — deliberate: handles
    /// are shared, so removing on a font_size change could free a font a
    /// sibling control still wears. Growth is bounded by the DISTINCT
    /// pixel heights ever requested (renders and patches all hit the
    /// cache — the key is the rounded integer, so drifting fractional
    /// sizes collapse), not by render count. Only worth revisiting if an
    /// app requests thousands of distinct heights in one session — then
    /// evict keys the current tree's font_size props don't reference.
    fonts: RefCell<HashMap<i32, HFONT>>,
}

/// WM_SETFONT fonts must outlive the controls that wear them, so this
/// drops root_widget (Widget::drop destroys the windows) BEFORE freeing
/// the fonts the controls reference.
impl Drop for AppState {
    fn drop(&mut self) {
        self.root_widget = None;
        for font in self.fonts.get_mut().values() {
            unsafe {
                let _ = DeleteObject((*font).into());
            }
        }
    }
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

        // lists get their real size from arrange — now push each list's
        // row height to the control (the small-image-list trick) and keep
        // its single column matched to its width (report-view columns
        // don't follow resizes, and the column is the rows' hit-test area,
        // not just paint box)
        if let Some(widget) = self.root_widget.as_mut() {
            sync_lists(widget);
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
                let hwnd = create_div(parent, background);
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
            ElementType::Text(text) => {
                // SS_NOTIFY lets accessibility tools (and the mouse)
                // interact with the label; SS_LEFT is the default (0).
                let hwnd = self.create_control(w!("static"), text, 0x0100, parent); // SS_NOTIFY-0x0100
                self.apply_font(hwnd, el);
                Widget::Label { hwnd }
            }
            ElementType::Button => {
                // Handlers live on widgets (the InputDelegate model,
                // everywhere): WM_COMMAND's lparam IS the control's hwnd, so
                // the event source resolves itself. The cacao side's id
                // registry exists only for its Send boundary.
                let hwnd = self.create_control(
                    w!("button"),
                    &button_label(el),
                    BS_PUSHBUTTON as u32 | WS_TABSTOP.0,
                    parent,
                );
                self.apply_font(hwnd, el);
                Widget::Button {
                    hwnd,
                    on_click: match el.handlers.get("on_click") {
                        Some(Handler::Simple(event)) => Some(event.clone()),
                        _ => None,
                    },
                }
            }
            ElementType::Input => {
                // Like the macOS InputDelegate: the widget OWNS its
                // on_change handler. Payload-carrying events live with their
                // widget (the text comes from the control at fire time); the
                // id registry is for payload-free dispatch.
                let hwnd = self.create_control(w!("EDIT"), "", WS_BORDER.0 | WS_TABSTOP.0, parent);
                self.apply_font(hwnd, el);
                if let Some(value) = el.props.get_string(PropType::Value) {
                    set_window_text(hwnd, value);
                }
                if let Some(cue) = el.props.get_string(PropType::Placeholder) {
                    set_cue_banner(hwnd, cue);
                }
                Widget::Input {
                    hwnd,
                    on_change: el.handlers.get("on_change").cloned(),
                }
            }
            ElementType::List => {
                // The snapshot IS the datasource — same contract as the
                // macOS delegate: built at render, served to visible rows.
                let rows = reconcile::snapshot_rows(&self.state, el);
                let row_height = {
                    let dc = DeviceContext::from(parent);
                    rows.iter().map(|r| paint::natural_size(dc.hdc, r).1).max()
                }
                .unwrap_or(20);
                let list_hwnd = create_list(parent, rows.len());
                Widget::List {
                    hwnd: list_hwnd,
                    rows,
                    row_height,
                    // the row-height image list is deliberately NOT applied
                    // here: the widget still sits at its 10x10 creation size,
                    // and LVM_SETIMAGELIST at that size permanently offsets
                    // the item grid (a blank band above row 0 — and it can't
                    // be healed later). sync_lists applies it after
                    // arrange() has given the list its real size.
                    image_list: None,
                }
            }
            ElementType::Image => {
                todo!("win32: Image not yet implemented")
            }
            ElementType::Component(_) => {
                unreachable!("component elements are expanded before mounting")
            }
        }
    }

    /// Creates a child control. `style` is the class-specific style bits
    /// (the windows crate types these inconsistently — i32, STATIC_STYLES —
    /// so they arrive raw and join WS_CHILD | WS_VISIBLE here).
    fn create_control(&self, class: PCWSTR, text: &str, style: u32, parent: HWND) -> HWND {
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
                None, // child id: unused — events resolve by hwnd
                Some(HINSTANCE(GetModuleHandleW(None).unwrap().0)),
                None,
            )
            .expect("CreateWindowExW control")
        }
    }

    /// WM_SETFONT: the control wears the font its element's font_size prop
    /// asks for — POINTS (like macOS's `Font::system`) converted to pixels
    /// at this DC's DPI. The same math TextGuard paints list rows with (see
    /// font.rs), so what's measured is what's drawn. Handles come from a
    /// size-keyed cache (equal sizes share one HFONT); without the prop
    /// nothing is sent and the control keeps its default font.
    fn apply_font(&self, hwnd: HWND, el: &Element) {
        let Some(points) = el.props.get_float(PropType::FontSize) else {
            return; // no font_size: the control keeps its default font
        };
        let pixels = unsafe {
            let hdc = GetDC(Some(hwnd));
            let pixels = scaled_pixels(points, hdc);
            let _ = ReleaseDC(Some(hwnd), hdc);
            pixels
        };
        let font = *self
            .fonts
            .borrow_mut()
            .entry(pixels)
            .or_insert_with(|| create_font(pixels));
        unsafe {
            let _ = SendMessageW(
                hwnd,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)), // redraw now
            );
        }
    }

    /// Reconcile the font like any prop: a patched control re-wears its
    /// font when (and only when) font_size actually changed — an unchanged
    /// size is a cache hit. Removing the prop keeps the last font in place
    /// (the control's original default is gone after our first WM_SETFONT)
    /// — TODO(font): flip back to the stock GUI font then.
    fn refresh_font(&self, old_el: &Element, new_el: &Element, hwnd: HWND) {
        if old_el.props.get_float(PropType::FontSize) != new_el.props.get_float(PropType::FontSize)
            && new_el.props.get_float(PropType::FontSize).is_some()
        {
            self.apply_font(hwnd, new_el);
        }
    }

    /// Reconciliation, same shape as the macOS `patch`: reuse widgets whose
    /// elements line up, refresh props and handlers in place.
    fn patch(&self, widget: &mut Widget, old_el: &Element, new_el: &Element) {
        match (widget, &new_el.element_type) {
            (widget @ Widget::Container { .. }, ElementType::Window | ElementType::Div) => {
                self.patch_container(widget, old_el, new_el);
            }
            (Widget::Button { hwnd, on_click }, ElementType::Button) => {
                self.refresh_font(old_el, new_el, *hwnd);
                // reconcile the title
                if button_label(old_el) != button_label(new_el) {
                    let title = get_utf16_vec(&button_label(new_el));
                    unsafe {
                        let _ = SetWindowTextW(*hwnd, PCWSTR(title.as_ptr()));
                    }
                }
                // the handler is a widget field now — a write keeps it
                // current (the cacao side's registry-refresh, deleted)
                *on_click = match new_el.handlers.get("on_click") {
                    Some(Handler::Simple(event)) => Some(event.clone()),
                    _ => None,
                };
            }
            (Widget::Label { hwnd }, ElementType::Text(text)) => {
                self.refresh_font(old_el, new_el, *hwnd);
                // display-only: no cursor to protect, just refresh
                let title = get_utf16_vec(text);
                unsafe {
                    let _ = SetWindowTextW(*hwnd, PCWSTR(title.as_ptr()));
                }
            }
            (Widget::Input { hwnd, on_change }, ElementType::Input) => {
                self.refresh_font(old_el, new_el, *hwnd);
                // controlled value: write only what actually differs — the
                // caret must never move under the user (same contract as the
                // cacao side)
                if old_el.props.get_string(PropType::Value)
                    != new_el.props.get_string(PropType::Value)
                    && let Some(value) = new_el.props.get_string(PropType::Value)
                    && get_window_text(*hwnd) != value
                {
                    set_window_text(*hwnd, value);
                }
                if old_el.props.get_string(PropType::Placeholder)
                    != new_el.props.get_string(PropType::Placeholder)
                    && let Some(cue) = new_el.props.get_string(PropType::Placeholder)
                {
                    set_cue_banner(*hwnd, cue);
                }
                // the handler lives on the widget now — refreshing it is a
                // field write (it's always current, same as the delegate)
                *on_change = new_el.handlers.get("on_change").cloned();
            }
            (
                Widget::List {
                    hwnd,
                    rows,
                    row_height,
                    image_list,
                },
                ElementType::List,
            ) => {
                // refresh the snapshot wholesale (the render's rows) and
                // tell the control the new count — it re-asks for whatever
                // is visible, same as the macOS reload() pass
                *rows = reconcile::snapshot_rows(&self.state, new_el);
                let measured = {
                    let dc = DeviceContext::from(*hwnd);
                    rows.iter().map(|r| paint::natural_size(dc.hdc, r).1).max()
                }
                .unwrap_or(20);
                if measured != *row_height {
                    *row_height = measured;
                    // a changed row height rides in on a fresh image list —
                    // drop the old one (None = "pending") and let
                    // sync_lists re-apply it post-arrange, the same
                    // deferral as at mount: never while off-layout
                    if let Some(old) = image_list.take() {
                        unsafe {
                            let _ = ImageList_Destroy(Some(old));
                        }
                    }
                }
                unsafe {
                    SendMessageW(*hwnd, LVM_SETITEMCOUNT, Some(WPARAM(rows.len())), None);
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
            // the CONTAINER's hwnd, not the main window: a new child of a
            // Div belongs to the div (mount_element parents it there at
            // first mount — patch must agree or the control lands on the
            // main window and gets positioned in the wrong space)
            |child_el| self.mount_element(*hwnd, child_el),
        );
    }

    /// App messages arrive erased (so AppState stays non-generic): the
    /// adapter built in `run` downcasts them back to M.
    pub fn deliver_app_message(&mut self, message: Box<dyn Any + Send>) {
        (self.on_app_message)(&self.state, message);
    }
}

/// BN_CLICKED: the widget owns the handler (same as Input) — find it by its
/// control's hwnd and run it, then re-render. `try_borrow` because messages
/// can arrive mid-render (our own SetWindowPos fires WM_SIZE synchronously)
/// — a render in flight wins.
fn click_and_render(app: &RefCell<AppState>, button: HWND) {
    if let Ok(mut app) = app.try_borrow_mut() {
        let handler = app
            .root_widget
            .as_mut()
            .and_then(|root| find_button_handler(root, button));
        if let Some(event) = handler {
            event.fire(&app.state);
            app.render();
        }
    }
}

fn find_button_handler(widget: &mut Widget, hwnd: HWND) -> Option<Event> {
    match widget {
        Widget::Button { hwnd: h, on_click } if *h == hwnd => on_click.clone(),
        Widget::Container { children, .. } => children
            .iter_mut()
            .find_map(|child| find_button_handler(child, hwnd)),
        _ => None,
    }
}

/// EN_CHANGE: the widget owns the handler (the InputDelegate model) — find
/// it by its control's hwnd and run it with the text the control holds.
fn text_change_and_render(app: &RefCell<AppState>, edit: HWND) {
    if let Ok(mut app) = app.try_borrow_mut() {
        let handler = app
            .root_widget
            .as_mut()
            .and_then(|root| find_input_handler(root, edit));
        if let Some(Handler::Change(handler)) = handler {
            handler(&app.state, get_window_text(edit));
            app.render();
        }
    }
}

fn find_input_handler(widget: &mut Widget, hwnd: HWND) -> Option<Handler> {
    match widget {
        Widget::Input { hwnd: h, on_change } if *h == hwnd => on_change.clone(),
        Widget::Container { children, .. } => children
            .iter_mut()
            .find_map(|child| find_input_handler(child, hwnd)),
        _ => None,
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
                // child controls report here; lparam is the control's hwnd —
                // the event source identifies itself, so handlers resolve
                // through the widget tree and no dispatch id is involved
                let code = ((wparam.0 >> 16) & 0xffff) as u32;
                if code == BN_CLICKED {
                    click_and_render(app, HWND(lparam.0 as *mut _));
                    return LRESULT(0);
                }
                if code == EN_CHANGE {
                    text_change_and_render(app, HWND(lparam.0 as *mut _));
                    return LRESULT(0);
                }
            }
            WM_NOTIFY => {
                // child controls report here too — NMHDR.hwndFrom is the
                // event source (the same model as Input/Button clicks).
                // The custom-draw return chain must propagate, so this
                // returns rather than falling through.
                return notify(app, lparam);
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

/// Sets a window's text from a Rust string.
fn set_window_text(hwnd: HWND, text: &str) {
    let wide = get_utf16_vec(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
    }
}

/// The placeholder cue (EM_SETCUEBANNER). Silently absent on pre-comctl32-6
/// setups — acceptable for a placeholder.
fn set_cue_banner(hwnd: HWND, text: &str) {
    let wide = get_utf16_vec(text);
    unsafe {
        let _ = SendMessageW(
            hwnd,
            EM_SETCUEBANNER,
            Some(WPARAM(1)),
            Some(LPARAM(wide.as_ptr() as isize)),
        );
    }
}

/// A control's full text, unbounded (the EM_GETLINE dance this replaces
/// capped at 256 chars and needed a length-prefix buffer).
fn get_window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd) as usize;
        let mut buffer = vec![0u16; len + 1];
        let n = GetWindowTextW(hwnd, &mut buffer) as usize;
        String::from_utf16_lossy(&buffer[..n])
    }
}

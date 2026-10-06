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
    mem,
    rc::Rc,
    sync::atomic::{AtomicIsize, Ordering},
};

use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::{
            CreateCompatibleBitmap, CreateSolidBrush, DeleteObject, FillRect, GetDC, HBITMAP,
            HBRUSH, HDC, InvalidateRect, SetBkMode, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                CDDS_ITEMPOSTPAINT, CDDS_ITEMPREPAINT, CDDS_PREPAINT, CDRF_DODEFAULT,
                CDRF_NOTIFYITEMDRAW, CDRF_NOTIFYPOSTERASE, CDRF_NOTIFYPOSTPAINT, EM_SETCUEBANNER,
                HIMAGELIST, ICC_LISTVIEW_CLASSES, ILC_COLOR32, INITCOMMONCONTROLSEX, ImageList_Add,
                ImageList_Create, ImageList_Destroy, InitCommonControlsEx, LVCF_FMT, LVCF_MINWIDTH,
                LVCF_WIDTH, LVCFMT_LEFT, LVCOLUMNW, LVM_GETITEMRECT, LVM_INSERTCOLUMN,
                LVM_SETCOLUMNWIDTH, LVM_SETEXTENDEDLISTVIEWSTYLE, LVM_SETIMAGELIST,
                LVM_SETITEMCOUNT, LVN_ITEMCHANGED, LVN_ODCACHEHINT, LVN_ODSTATECHANGED,
                LVS_EX_FULLROWSELECT, LVS_NOCOLUMNHEADER, LVS_OWNERDATA, LVS_REPORT,
                LVS_SHOWSELALWAYS, LVSIL_SMALL, NM_CUSTOMDRAW, NMCUSTOMDRAW, NMHDR,
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
    win32::dc,
};

use super::{
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
const WM_GET_BG_BRUSH: u32 = WM_APP + 3;

const CLASS_NAME: PCWSTR = w!("wo_mainwin");
const DIV_CLASS: PCWSTR = w!("wo_div");

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

/// Common controls (the list-view) need explicit init.
fn init_common_controls() {
    unsafe {
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES,
        });
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

/// Windows color mapping — the Apple system (light) colors the cacao
/// `color()` uses, as COLORREF (0x00BBGGRR), so the same ui! code looks
/// right on both platforms.
pub(crate) fn color_ref(name: &str) -> COLORREF {
    match name {
        "blue" => COLORREF(0x00FF_7A00),  // rgb(0, 122, 255)
        "red" => COLORREF(0x0030_3BFF),   // rgb(255, 59, 48)
        "green" => COLORREF(0x0058_D130), // rgb(48, 209, 88)
        "gray" => COLORREF(0x0093_8E8E),  // rgb(142, 142, 147)
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
    root_widget: Option<Widget>,
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
                // SS_NOTIFY lets accessibility tools (and the mouse)
                // interact with the label; SS_LEFT is the default (0).
                hwnd: self.create_control(w!("static"), text, 0x0100, parent), // SS_NOTIFY-0x0100
            },
            ElementType::Button => {
                // Handlers live on widgets (the InputDelegate model,
                // everywhere): WM_COMMAND's lparam IS the control's hwnd, so
                // the event source resolves itself. The cacao side's id
                // registry exists only for its Send boundary.
                Widget::Button {
                    hwnd: self.create_control(
                        w!("button"),
                        &button_label(el),
                        BS_PUSHBUTTON as u32 | WS_TABSTOP.0,
                        parent,
                    ),
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
                    let dc = dc::DeviceContext::get_dc(parent);
                    rows.iter().map(|r| paint::natural_size(dc.hdc, r).1).max()
                }
                .unwrap_or(20);
                let list_hwnd = self.create_list(parent, rows.len());
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

    fn set_row_height(list: HWND, himl_slot: &mut Option<HIMAGELIST>, height: i32) {
        unsafe {
            let himl = ImageList_Create(1, height, ILC_COLOR32, 1, 1);
            let hbm = CreateCompatibleBitmap(GetDC(Some(list)), 1, height);
            ImageList_Add(himl, hbm, Some(HBITMAP::default()));
            _ = DeleteObject(hbm.into());

            let _ = SendMessageW(
                list,
                LVM_SETIMAGELIST,
                Some(WPARAM(LVSIL_SMALL as usize)),
                Some(LPARAM(himl.0 as isize)),
            );
            if let Some(old) = *himl_slot {
                let _ = ImageList_Destroy(Some(old));
            }
            *himl_slot = Some(himl);
        }
    }

    /// A Div's child window — its own background brush rides along as
    /// lpParam (null = inherit the parent's).
    fn create_div(&self, parent: HWND, background: Option<HBRUSH>) -> HWND {
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

    /// A virtual list-view — the control owns only the COUNT
    /// (LVS_OWNERDATA); rows come from the snapshot as they scroll into
    /// view. Single-column, headerless report view = our list look.
    fn create_list(&self, parent: HWND, count: usize) -> HWND {
        let style = LVS_REPORT
            | LVS_OWNERDATA
            | LVS_NOCOLUMNHEADER
            | LVS_SHOWSELALWAYS
            | WS_TABSTOP.0
            | WS_BORDER.0
            | WS_CHILD.0
            | WS_VISIBLE.0;
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("SysListView32"),
                w!(""),
                WINDOW_STYLE(style),
                0,
                0,
                10,
                10, // arrange() positions it before anything is visible
                Some(parent),
                None,
                Some(HINSTANCE(GetModuleHandleW(None).unwrap().0)),
                None,
            )
            .expect("CreateWindowExW list")
        };
        unsafe {
            // Full-row select: without it, hit-testing is confined to the
            // item's icon+label box (LVIR_SELECTBOUNDS) — and for a virtual
            // item with no text that box is a ~48px stub, so clicks past it
            // never select and the highlight is a sliver. With this style
            // the whole row is the hit target.
            SendMessageW(
                hwnd,
                LVM_SETEXTENDEDLISTVIEWSTYLE,
                Some(WPARAM(LVS_EX_FULLROWSELECT as usize)),
                Some(LPARAM(LVS_EX_FULLROWSELECT as isize)),
            );
            SendMessageW(hwnd, LVM_SETITEMCOUNT, Some(WPARAM(count)), None);
        }
        // A report-view item lives in COLUMN SPACE — with no columns the rows
        // have zero width: nothing to hit-test, so clicks never select and no
        // selection notifications fire. One full-width column is the list's
        // body; its width is synced to the control after arrange.
        let mut column = LVCOLUMNW {
            mask: LVCF_FMT | LVCF_WIDTH | LVCF_MINWIDTH,
            fmt: LVCFMT_LEFT,
            cx: 600,
            cxMin: 600,
            ..Default::default()
        };
        unsafe {
            SendMessageW(
                hwnd,
                LVM_INSERTCOLUMN,
                Some(WPARAM(0)),
                Some(LPARAM(&mut column as *mut LVCOLUMNW as isize)),
            );
        }

        hwnd
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

    /// Reconciliation, same shape as the macOS `patch`: reuse widgets whose
    /// elements line up, refresh props and handlers in place.
    fn patch(&self, widget: &mut Widget, old_el: &Element, new_el: &Element) {
        match (widget, &new_el.element_type) {
            (widget @ Widget::Container { .. }, ElementType::Window | ElementType::Div) => {
                self.patch_container(widget, old_el, new_el);
            }
            (Widget::Button { hwnd, on_click }, ElementType::Button) => {
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
                // display-only: no cursor to protect, just refresh
                let title = get_utf16_vec(text);
                unsafe {
                    let _ = SetWindowTextW(*hwnd, PCWSTR(title.as_ptr()));
                }
            }
            (Widget::Input { hwnd, on_change }, ElementType::Input) => {
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
                    let dc = dc::DeviceContext::get_dc(*hwnd);
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
            |child_el| self.mount_element(self.hwnd, child_el),
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

/// The snapshot rows for the list with this hwnd — the datasource the
/// painting work draws from.
// `Vec<Box<Element>>` is the tree's node currency (see `Widget::List::rows`)
#[allow(clippy::vec_box)]
fn find_list_rows(widget: &mut Widget, hwnd: HWND) -> Option<&Vec<Box<Element>>> {
    match widget {
        Widget::List { hwnd: h, rows, .. } if *h == hwnd => Some(rows),
        Widget::Container { children, .. } => children
            .iter_mut()
            .find_map(|child| find_list_rows(child, hwnd)),
        _ => None,
    }
}

/// Post-arrange list sync. The row height rides on a small image list
/// (the ObjectListView trick — rows are uniform in report view), and it
/// must only be applied once the list has its real size: LVM_SETIMAGELIST
/// on a list still at its 10x10 creation size computes the item grid from
/// that tiny view and the blank band above row 0 sticks forever. mount and
/// patch therefore leave `image_list` as None ("pending"); applying it
/// here — right after arrange() — lands it on a properly sized control.
fn sync_lists(widget: &mut Widget) {
    match widget {
        Widget::List {
            hwnd,
            row_height,
            image_list,
            ..
        } => {
            if image_list.is_none() {
                AppState::set_row_height(*hwnd, image_list, *row_height);
            }
            unsafe {
                let mut rect = RECT::default();
                let _ = GetClientRect(*hwnd, &mut rect);
                SendMessageW(
                    *hwnd,
                    LVM_SETCOLUMNWIDTH,
                    Some(WPARAM(0)),
                    Some(LPARAM((rect.right - rect.left) as isize)),
                );
            }
        }
        Widget::Container { children, .. } => children.iter_mut().for_each(sync_lists),
        _ => {}
    }
}

/// WM_NOTIFY from a child control. The custom-draw stage chain is the
/// seam where row painting plugs in — the plumbing is here, the pixels
/// are yours.
fn notify(app: &RefCell<AppState>, lparam: LPARAM) -> LRESULT {
    unsafe {
        let hdr = &*(lparam.0 as *const NMHDR);
        match hdr.code {
            NM_CUSTOMDRAW => {
                let draw = &*(lparam.0 as *const NMCUSTOMDRAW);
                match draw.dwDrawStage {
                    CDDS_PREPAINT => {
                        // LRESULT(CDRF_NOTIFYITEMDRAW as isize)
                        LRESULT((CDRF_NOTIFYPOSTPAINT | CDRF_NOTIFYITEMDRAW) as isize)
                    }
                    CDDS_ITEMPREPAINT => {
                        // TODO(painting): paint the row here from
                        // find_list_rows(app…, hdr.hwndFrom)[draw.dwItemSpec]
                        // — stack boxes via stack::natural/arrange, pixels
                        // via DrawTextW/FillRect. Return
                        // CDRF_SKIPDEFAULT once we own the row's painting.
                        LRESULT((CDRF_NOTIFYPOSTPAINT | CDRF_NOTIFYPOSTERASE) as isize)
                    }
                    CDDS_ITEMPOSTPAINT => {
                        // the row rect — the control's `rc` is NOT filled
                        // for list-view custom draw, query it — and the
                        // row's snapshot element to paint
                        let row = draw.dwItemSpec;
                        let mut rect = RECT::default();
                        SendMessageW(
                            hdr.hwndFrom,
                            LVM_GETITEMRECT,
                            Some(WPARAM(row)),
                            Some(LPARAM(&mut rect as *mut RECT as isize)),
                        );
                        let area = Rect {
                            x: rect.left,
                            y: rect.top,
                            w: rect.right - rect.left,
                            h: rect.bottom - rect.top,
                        };
                        if let Ok(mut app) = app.try_borrow_mut() {
                            if let Some(element) = app
                                .root_widget
                                .as_mut()
                                .and_then(|root| find_list_rows(root, hdr.hwndFrom))
                                .and_then(|rows| rows.get(row))
                            {
                                paint::paint_tree(draw.hdc, element, area);
                            }
                        } else {
                            // A render is in flight (it borrows the app), and
                            // renders resize this very control — the redraw
                            // that brought us here is their own SetWindowPos
                            // or LVM_SETCOLUMNWIDTH. Painting now would read
                            // a half-updated tree, so the row is skipped — but
                            // this synchronous pass VALIDATES the control, so
                            // no WM_PAINT would follow and the list would stay
                            // blank (virtual rows draw empty). Re-invalidate:
                            // the queued WM_PAINT lands after the render
                            // returns and paints the rows for real.
                            let _ = InvalidateRect(Some(hdr.hwndFrom), None, false);
                        }
                        LRESULT(CDRF_DODEFAULT as isize)
                    }
                    _ => LRESULT(CDRF_DODEFAULT as isize),
                }
            }
            LVN_ODCACHEHINT => {
                // TODO(painting/live rows): the visible range just changed
                // (NMLVCACHEHINT iFrom..iTo) — the prefetch window the
                // macOS side covers with its dequeue pool.
                LRESULT(0)
            }
            LVN_ITEMCHANGED => {
                // TODO(on_select): selection changed — a future
                // Handler::Select rides here.
                LRESULT(0)
            }
            LVN_ODSTATECHANGED => LRESULT(0),
            _ => LRESULT(0),
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

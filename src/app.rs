use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
    sync::Arc,
};

#[cfg(target_os = "macos")]
use cacao::appkit::App;
use cacao::{
    appkit::{
        AppDelegate,
        menu::Menu,
        window::{Window, WindowConfig, WindowStyle},
    },
    button::Button,
    color::Color,
    core_graphics::display::{CGRect, CGSize},
    foundation::{NSString, nil},
    image::ImageView,
    input::TextField,
    layout::{Layout, LayoutAnchorX, LayoutAnchorY, LayoutConstraint},
    listview::ListView,
    notification_center::Dispatcher,
    objc::{class, msg_send, runtime::Object, sel, sel_impl},
    text::{Font, Label},
    view::View,
};

use crate::{
    component::Component,
    element::{BlitFrame, Element, ElementType, window_spec},
    input::InputDelegate,
    layout::{Direction, FlexStyle},
    listview::ReactiveListView,
    state::{Ctx, Event, Handler, State},
    widgets::{Widget, compatible, flex_changed},
    window::WindowProxy,
};

/// Application should automatically know what system it's running on, and so instantiate the correct windowing library.
/// For now, this is just cacao (TODO - add win32 and a testing harness)
pub struct Application {}

impl Application {
    /// Runs the app. `on_app_message` interprets the app's own messages
    /// (the `M` in `Message<M>`) with read access to state; the framework
    /// re-renders after each one.
    /// # Examples
    ///
    /// ```no_run
    /// # use window_of_opportunity::{
    /// #     app::{Application, dispatch},
    /// #     component::Component,
    /// #     element::{BlitFrame, Element},
    /// #     state::Ctx,
    /// #     ui,
    /// # };
    /// # use std::sync::Arc;
    /// // The app specific message type
    /// enum MyAppMsg {
    ///     FrameUpdate {
    ///         width: usize,
    ///         height: usize,
    ///         samples: u64,
    ///         pixels: Arc<Vec<u8>>,
    ///     }
    /// }
    ///
    /// # // MainWindow is our custom component with a `render` function.
    /// # #[derive(Debug)]
    /// # struct MainWindow {}
    ///
    /// # impl Component for MainWindow {
    /// #    fn render(&self, ctx: &window_of_opportunity::state::Ctx, _children: Vec<Box<window_of_opportunity::element::Element>>) -> Box<window_of_opportunity::element::Element> {
    /// #        let (w, h) = ctx.use_state("window/size", || (640., 820.));
    /// #        ui! {
    /// #            Window width(w) height(h)
    /// #                on_resize(|state, w, h|
    /// #                    state.update("window/size", |s: &mut (f64, f64)| *s = (w, h))) {
    /// #                { Div direction("column") gap(10.) grow(true) padding(16.) background("green") {
    /// #                    { Button {{ Text "Click Me" }} }
    /// #                }}
    /// #            }
    /// #        }
    /// #    }
    /// # }
    ///
    /// fn main() {
    ///     // MainWindow is a custom component with a `render` function
    ///     let main_window = Box::new(MainWindow {});
    ///     let app = Application {};
    ///
    ///     app.run(main_window, |state, message| match message {
    ///         MyAppMsg::FrameUpdate {
    ///             width,
    ///             height,
    ///             samples,
    ///             pixels,
    ///         } => {
    ///             state.update("trace/frame", |f: &mut BlitFrame| {
    ///                 f.width = width;
    ///                 f.height = height;
    ///                 f.samples = samples;
    ///                 f.pixels = pixels;
    ///                 f.version += 1;
    ///             });
    ///         }
    ///     });
    /// }
    /// ```
    /// If you don't want any custom messages:
    /// ```no_run
    /// # use window_of_opportunity::{
    /// #     app::{Application, dispatch},
    /// #     component::Component,
    /// #     element::{BlitFrame, Element},
    /// #     state::Ctx,
    /// #     ui,
    /// # };
    /// # #[derive(Debug)]
    /// # struct Window {}
    /// #
    /// # impl Component for Window {
    /// #     fn render(
    /// #         &self,
    /// #         _ctx: &window_of_opportunity::state::Ctx,
    /// #         _children: Vec<Box<window_of_opportunity::element::Element>>,
    /// #     ) -> Box<window_of_opportunity::element::Element> {
    /// #         ui! {
    /// #             Window {}
    /// #             }
    /// #         
    /// #     }
    /// # }
    ///
    /// fn main() {
    ///   let window = Box::new(Window {});
    ///   let app = Application {};
    ///
    ///   app.run(window, |_state, _message: ()| ())
    /// }
    /// ```
    pub fn run<M: Send + Sync + 'static>(
        &self,
        root: Box<dyn Component>,
        on_app_message: impl Fn(&State, M) + 'static,
    ) {
        #[cfg(target_os = "macos")]
        App::new("com.hello.world", ReactApp::<M>::new(root, on_app_message)).run();
    }
}

pub struct AppState {
    /// The mounted widget tree. Dropping this drops every original view, and
    /// each `View::Drop` removes itself from its superview — so teardown is
    /// just `root_widget = None`.
    root_widget: Option<Widget>,

    /// the hooks store: keyed, typed slots
    pub state: State,

    /// render inputs
    root: Box<dyn Component>,
    window: Window<WindowProxy>,
    content: View,

    /// shared weak back-reference to the Rc wrapping this AppState — handed
    /// to delegates (window proxy, text fields) so they can run the loop
    app_weak: Rc<RefCell<Option<Weak<RefCell<AppState>>>>>,

    /// "put a widget event on the main queue" — injected at construction,
    /// because only there can the concrete App/delegate types be named.
    /// This is what keeps AppState (and everything under it) non-generic.
    dispatch_event: Arc<dyn Fn(usize) + Send + Sync>,

    /// the window size the last render requested, so a re-render with an
    /// unchanged request doesn't clobber a user resize (the controlled-
    /// component rule: writes happen when the request changes, not when the
    /// actual drifts)
    last_requested_size: Cell<Option<(f64, f64)>>,

    /// the previous render's expanded element tree — the diff target for
    /// reconciliation
    last_tree: Option<Box<Element>>,

    /// the root container's pins to the content view — regenerated each
    /// render (the root is usually reused, so the old ones are deactivated)
    root_pins: RefCell<Vec<LayoutConstraint>>,

    /// live events, by dispatch id. Ids are never reused: a stale id from a
    /// previous tree still resolves — and running its updater is harmless,
    /// since events address slots by key, not by widget identity. (The map
    /// grows by one entry per mounted handler per render — fine for now,
    /// prune it when diffing arrives.)
    handlers_by_id: RefCell<HashMap<usize, Event>>,

    /// the root element's on_resize handler, if it declared one — handed to
    /// the WindowProxy so user resizes flow through component logic
    pub resize_handler: RefCell<Option<Handler>>,
    next_handler_id: Cell<usize>,
}

impl AppState {
    /// The full render pipeline lives on AppState (not ReactApp) so the
    /// dispatcher can run it after every event: render, mount, layout, fit
    /// the window.
    pub fn render(&mut self) {
        let tree = self.root.render(&Ctx { state: &self.state }, vec![]);
        // Expand component nodes with the current state, so patch/mount
        // only ever see primitives.
        let tree = self.expand(&tree);
        #[cfg(feature = "debug_dump")]
        println!("{tree:#?}");

        let spec = window_spec(&tree);

        // hand the root's on_resize (if any) to the window proxy — resizes
        // flow through component logic, not a magic state key
        *self.resize_handler.borrow_mut() = tree.handlers.get("on_resize").cloned();

        // focus snapshot before reconciling (position + field pointer)
        let focus = self.focused_snapshot();

        // Phase 2: reconcile against the previous element tree instead of
        // teardown + remount. Widgets whose elements line up are REUSED —
        // the objc views, their text, selection and focus, all survive.
        let old_tree = self.last_tree.take();
        match (old_tree, self.root_widget.take()) {
            (Some(old_tree), Some(mut root_widget)) => {
                // the widget is taken OUT during patching so the &mut and
                // patch's &self don't alias
                self.patch(&self.content, &mut root_widget, &old_tree, &tree);
                self.root_widget = Some(root_widget);
            }
            (_, root_widget) => {
                // first render: mount fresh
                let mut root_widget =
                    root_widget.unwrap_or_else(|| self.mount_element(&self.content, &tree));
                self.layout_node(&tree, &mut root_widget);
                self.root_widget = Some(root_widget);
            }
        }

        // Measured once per render: the real title-bar height (not a
        // hardcoded constant) feeds the root pin and the size math below.
        let inset = self.titlebar_inset();

        // The root always starts top-left, below the title bar. A specified
        // axis is pinned to the window — the window drives the layout. An
        // unspecified axis is left unpinned so the content drives it, and
        // the window is fitted to the result below. The root is usually
        // reused, so the previous render's pins are deactivated first.
        {
            let mut pins = self.root_pins.borrow_mut();
            LayoutConstraint::deactivate(&pins);
            pins.clear();
            if let Some(Widget::Container { view, .. }) = self.root_widget.as_ref() {
                pins.push(
                    view.top
                        .constraint_equal_to(&self.content.top)
                        .offset(inset),
                );
                pins.push(view.leading.constraint_equal_to(&self.content.leading));
                if spec.width.is_some() {
                    pins.push(view.trailing.constraint_equal_to(&self.content.trailing));
                }
                if spec.height.is_some() {
                    pins.push(view.bottom.constraint_equal_to(&self.content.bottom));
                }
                LayoutConstraint::activate(&pins);
            }
        }

        self.window.set_title(&spec.title);

        // Size the window. Explicit props win; missing axes hug the content.
        // fittingSize is the smallest size that satisfies the constraint
        // system — for an unpinned axis that's the content's natural size.
        let mut content_w = spec.width;
        let mut content_h = spec.height.map(|h| h + inset);
        if content_w.is_none() || content_h.is_none() {
            let fit: CGSize = self
                .content
                .objc
                .get(|obj| unsafe { msg_send![obj, fittingSize] });
            content_w = content_w.or(Some(fit.width));
            content_h = content_h.or(Some(fit.height));
        }
        // Controlled-component reconciliation: only write the window's size
        // when this render requests a DIFFERENT size than the last render
        // did. A user resize in between is the user's business.
        let requested = (content_w.unwrap(), content_h.unwrap());
        if self.last_requested_size.replace(Some(requested)) != Some(requested) {
            self.window.set_content_size(requested.0, requested.1);
        }

        // Pin the previously unpinned axes so the root now fills the window.
        {
            let mut pins = self.root_pins.borrow_mut();
            if let Some(Widget::Container { view, .. }) = self.root_widget.as_ref() {
                if spec.width.is_none() {
                    pins.push(view.trailing.constraint_equal_to(&self.content.trailing));
                }
                if spec.height.is_none() {
                    pins.push(view.bottom.constraint_equal_to(&self.content.bottom));
                }
                if !pins.is_empty() {
                    LayoutConstraint::activate(&pins);
                }
            }
        }

        // Focus: only intervene if the focused field was destroyed and an
        // input now sits in its place — a field that survived reconciliation
        // keeps its cursor exactly where the user left it.
        if let Some((path, old_field_ptr)) = focus {
            self.restore_focus_if_replaced(&path, old_field_ptr);
        }

        // (debug) Force a layout pass and dump solved geometry. Delete or
        // gate behind a flag when this gets boring.
        #[cfg(feature = "debug_dump")]
        if let Some(root_widget) = self.root_widget.as_ref() {
            if let Widget::Container { view, .. } = root_widget {
                view.objc.with_mut(|obj| unsafe {
                    let _: () = msg_send![obj, layoutSubtreeIfNeeded];
                });
            }
            debug_dump(&tree, root_widget, 0);
        }

        // keep the expanded tree for the next render's diff
        self.last_tree = Some(tree);
    }

    /// Builds the row elements for a List element against current state —
    /// the snapshot the list delegate serves from in `item_for`. Runs once
    /// per render (mount or patch), never at display time. Count comes from
    /// the `rows(n)` prop; the handler from `on_display_item`.
    fn snapshot_rows(&self, el: &Element) -> Vec<Box<Element>> {
        let count: usize = el
            .props
            .get("rows")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let ctx = Ctx { state: &self.state };
        match el.handlers.get("on_display_item") {
            Some(Handler::ListItem(handler)) => (0..count).map(|i| handler(&ctx, i)).collect(),
            _ => Vec::new(),
        }
    }

    /// Mounts a row element into a list row's view. Rows are element trees
    /// like any other — the same mount+layout pipeline the tree pass uses.
    /// The returned Widget is kept by the row delegate: dropping it unmounts
    /// the row's content, which is how a recycled row is cleared.
    pub(crate) fn mount_row(&self, parent: &View, el: &Element) -> Widget {
        let mut widget = self.mount_element(parent, el);

        // Nothing else constrains the row root — pin it into the row view
        // itself. The bottom pin is optional (yields to the root's own
        // height), so automatic row heights fit the content.
        let a = widget.anchors();
        let pins = [
            a.top.constraint_equal_to(&parent.top),
            a.leading.constraint_equal_to(&parent.leading),
            a.trailing.constraint_equal_to(&parent.trailing),
            optional(a.bottom.constraint_equal_to(&parent.bottom)),
        ];
        LayoutConstraint::activate(&pins);

        self.layout_node(el, &mut widget);
        widget
    }

    /// Asks AppKit to re-query every list's datasource, once per completed
    /// render. `item_for` can fire while `render()` holds the state borrow
    /// (constraint work can trigger AppKit layout synchronously); those
    /// calls get an unconfigured row, and this pass — run with the borrow
    /// released — is what repaints them. Full reload every time; per-row
    /// diffing (reload_rows) is a later refinement.
    pub(crate) fn reload_lists(&self) {
        let Some(root) = self.root_widget.as_ref() else {
            return;
        };
        reload_lists_in(root);
    }

    /// Height of the title-bar strip the content view runs under. Windows are
    /// `FullSizeContentView`, so the content view spans the whole frame and
    /// the root must be pushed down by exactly this much. Measured via
    /// `contentLayoutRect` (the usable rectangle below the title bar) rather
    /// than hardcoded — the real height varies with macOS version, toolbars
    /// and accessibility settings.
    fn titlebar_inset(&self) -> f64 {
        unsafe {
            let content: *mut Object = msg_send![&*self.window.objc, contentView];
            let bounds: CGRect = msg_send![content, bounds];
            let layout: CGRect = msg_send![&*self.window.objc, contentLayoutRect];
            (bounds.size.height - layout.size.height).max(0.)
        }
    }

    /// Focus snapshot before reconciling: the focused input's position, plus
    /// the field's objc pointer — so restoration can tell a *replaced* field
    /// (restore focus, cursor to end) from a *survived* one (do nothing; its
    /// cursor is exactly where the user left it).
    fn focused_snapshot(&self) -> Option<(Vec<usize>, *mut Object)> {
        let root = self.root_widget.as_ref()?;

        let first_responder: *mut Object = unsafe { msg_send![&*self.window.objc, firstResponder] };
        if first_responder.is_null() {
            return None;
        }

        find_focused(root, first_responder, &mut Vec::new())
    }

    /// Hands focus to the input now sitting at `path` — but ONLY if the field
    /// that had focus was destroyed by reconciliation. A reused field never
    /// lost focus in the first place, and touching it would move the cursor.
    fn restore_focus_if_replaced(&self, path: &[usize], old_field_ptr: *mut Object) {
        let Some(root) = self.root_widget.as_ref() else {
            return;
        };
        let Some(Widget::Input(field)) = widget_at_path(root, path) else {
            return;
        };

        let field_ptr = field.objc.get(|obj| obj as *const Object as *mut Object);
        if field_ptr == old_field_ptr {
            return; // survived — focus and cursor are untouched
        }

        unsafe {
            let _: () = msg_send![&*self.window.objc, makeFirstResponder: field_ptr];

            // Focusing selects all by default — the next keystroke would wipe
            // the text. Move the insertion point to the end instead.
            let editor: *mut Object = msg_send![field_ptr, currentEditor];
            if !editor.is_null() {
                let text: *mut Object = msg_send![editor, string];
                let length: usize = msg_send![text, length];
                let _: () = msg_send![
                    editor,
                    setSelectedRange: NSRange { location: length, length: 0 }
                ];
            }
        }
    }

    /// Expands component elements with the current state: a component's
    /// render output is spliced into the tree, recursively. Runs before
    /// mounting so mount/layout operate on primitives only — and so it's the
    /// framework, not the ui! macro, that hands state to components.
    fn expand(&self, el: &Element) -> Box<Element> {
        match &el.element_type {
            ElementType::Component(component) => {
                let children = el.children.iter().map(|child| self.expand(child)).collect();
                let ctx = Ctx { state: &self.state };
                let rendered = component.render(&ctx, children);
                self.expand(&rendered)
            }
            _ => Box::new(Element {
                element_type: el.element_type.clone(),
                props: el.props.clone(),
                handlers: el.handlers.clone(),
                children: el.children.iter().map(|child| self.expand(child)).collect(),
            }),
        }
    }

    /// Creates the cacao views for an element tree and returns the mounted
    /// widget tree. Mounting does no layout — geometry is a separate pass so
    /// it has child handles available for constraint chaining.
    fn mount_element(&self, parent: &View, el: &Element) -> Widget {
        match &el.element_type {
            // Window gets a real backing container so it can act as the flex root
            // (and so Window-level props like padding have somewhere to live).
            ElementType::Window | ElementType::Div => {
                let view = View::default();

                if let Some(bg) = el.props.get("background") {
                    view.set_background_color(color(bg));
                }

                parent.add_subview(&view);

                let children = el
                    .children
                    .iter()
                    .map(|child| self.mount_element(&view, child))
                    .collect();

                Widget::Container {
                    view,
                    children,
                    constraints: Vec::new(),
                }
            }
            ElementType::Button => {
                let mut button = Button::new(&button_label(el));

                // Wire up on_click, if there is one. The handler is an Event
                // stored under a dispatch id; the button's action closure only
                // ever captures that id (a usize, always Send + Sync) — the
                // handler itself stays on the main-thread side of the queue.
                let mut handler_id = None;
                if let Some(handler) = el.handlers.get("on_click") {
                    match handler {
                        Handler::Simple(event) => {
                            let id = self.next_handler_id.replace(self.next_handler_id.get() + 1);
                            self.handlers_by_id.borrow_mut().insert(id, event.clone());
                            let send = self.dispatch_event.clone();
                            button.set_action(move || {
                                send(id);
                            });
                            handler_id = Some(id);
                        }
                        Handler::Resize(_) | Handler::Change(_) | Handler::ListItem(_) => {
                            println!("warning: on_click expects an Event (Ctx::set_state)")
                        }
                    }
                }

                parent.add_subview(&button);
                Widget::Button {
                    control: button,
                    handler_id,
                }
            }
            ElementType::Text(text) => {
                // A standalone Text mounts a Label — display text for state
                let label = Label::new();
                label.set_text(text);
                if let Some(txt_color) = el.props.get("color") {
                    label.set_text_color(color(txt_color));
                }
                if let Some(font_size) = el.props.get("font_size") {
                    let font_size: f64 = font_size.parse().unwrap();
                    let font = Font::system(font_size);
                    label.set_font(font);
                }

                parent.add_subview(&label);
                Widget::Label(label)
            }
            ElementType::List => {
                // Snapshot the rows NOW, against current state: AppKit pulls
                // rows from the delegate whenever it likes (including
                // mid-render), so it must serve from a snapshot, not from
                // state. Row count comes from the `rows(n)` prop.
                let delegate =
                    ReactiveListView::with(self.app_weak.clone(), self.snapshot_rows(el));
                let list_view = ListView::with(delegate);
                parent.add_subview(&list_view);
                Widget::List(list_view)
            }
            ElementType::Input => {
                let delegate = InputDelegate {
                    app: self.app_weak.clone(),
                    on_change: RefCell::new(el.handlers.get("on_change").cloned()),
                };
                let field = TextField::with(delegate);

                if let Some(value) = el.props.get("value") {
                    field.set_text(value);
                }
                if let Some(placeholder) = el.props.get("placeholder") {
                    field.set_placeholder_text(placeholder);
                }

                parent.add_subview(&field);
                Widget::Input(field)
            }
            ElementType::Image => {
                let view = ImageView::new();
                view.set_background_color(Color::SystemBlack);
                view.objc.with_mut(|obj| unsafe {
                    // NSImageScaleAxesIndependently — fill the view exactly
                    let _: () = msg_send![obj, setImageScaling: 2usize];
                });

                // display whatever the src slot currently holds
                let mut version = 0;
                if let Some(src_key) = el.props.get("src") {
                    let frame = self.state.use_state::<BlitFrame>(src_key, BlitFrame::empty);
                    if frame.version > 0 {
                        set_frame(&view, &frame);
                        version = frame.version;
                    }
                }

                parent.add_subview(&view);
                Widget::ImageView { view, version }
            }
            ElementType::Component(_) => {
                unreachable!("component elements are expanded before mounting")
            }
        }
    }

    /// Flexbox, expressed as AutoLayout equations.
    ///
    /// Column: children stack top-to-bottom (main axis) and stretch to the
    /// container's width (cross axis). Row: the same, rotated 90°.
    ///
    /// Something is always pinned to the container's far edge — a grow child
    /// if there is one, otherwise the last child. That rule is what keeps the
    /// solver unambiguous: an unsized container hugs its content (flexbox's
    /// `height: auto`) and a sized container stretches its last/grow child.
    fn layout_node(&self, el: &Element, widget: &mut Widget) {
        let Widget::Container {
            view,
            children,
            constraints,
        } = widget
        else {
            return; // leaves get an intrinsic size from AppKit
        };

        let generated = self.container_constraints(el, view, children);
        LayoutConstraint::activate(&generated);
        *constraints = generated;

        // recurse into child containers
        for (child_el, child_widget) in el.children.iter().zip(children.iter_mut()) {
            self.layout_node(child_el, child_widget);
        }
    }

    /// Generates the AutoLayout equations for ONE container: its own explicit
    /// size, the main-axis chain, the cross-axis pins, and the grow/hug rules
    /// for its direct children. The result is stored on the widget so that
    /// patching can deactivate exactly what it regenerates.
    ///
    /// Column: children stack top-to-bottom (main axis) and stretch to the
    /// container's width (cross axis). Row: the same, rotated 90°.
    ///
    /// Something is always pinned to the container's far edge — a grow child
    /// if there is one, otherwise the last child — as a required *inequality*
    /// (content must fit) plus an optional equality (hug) that yields to
    /// intrinsic sizes, so slack sits at the end of a sized container like
    /// flexbox's default.
    fn container_constraints(
        &self,
        el: &Element,
        view: &View,
        child_widgets: &[Widget],
    ) -> Vec<LayoutConstraint> {
        let style = FlexStyle::from_props(&el.props);
        let mut constraints = Vec::new();

        // Explicit size on the container itself — except for Window elements:
        // their size props size the window (handled in `render()`), not a view.
        if !matches!(el.element_type, ElementType::Window) {
            if let Some(w) = style.width {
                constraints.push(view.width.constraint_equal_to_constant(w));
            }
            if let Some(h) = style.height {
                constraints.push(view.height.constraint_equal_to_constant(h));
            }
        }

        match style.direction {
            Direction::Column => {
                let mut prev_bottom: Option<LayoutAnchorY> = None;
                let mut grow_bottom: Option<LayoutAnchorY> = None;

                for (child_el, child_widget) in el.children.iter().zip(child_widgets) {
                    let child_style = FlexStyle::from_props(&child_el.props);
                    let a = child_widget.anchors();

                    // main axis: stack top-to-bottom
                    constraints.push(match prev_bottom.take() {
                        None => a.top.constraint_equal_to(&view.top).offset(style.padding),
                        Some(prev) => a.top.constraint_equal_to(&prev).offset(style.gap),
                    });
                    prev_bottom = Some(a.bottom.clone());

                    // cross axis: stretch to the container's width
                    constraints.push(
                        a.leading
                            .constraint_equal_to(&view.leading)
                            .offset(style.padding),
                    );
                    constraints.push(
                        a.trailing
                            .constraint_equal_to(&view.trailing)
                            .offset(-style.padding),
                    );

                    if let Some(h) = child_style.height {
                        constraints.push(a.height.constraint_equal_to_constant(h));
                    }
                    if let Some(w) = child_style.width {
                        constraints.push(a.width.constraint_equal_to_constant(w));
                    }
                    if child_style.grow {
                        grow_bottom = Some(a.bottom);
                    }
                }

                let has_grow = grow_bottom.is_some();
                if let Some(bottom) = grow_bottom.or(prev_bottom) {
                    // flexbox containment: main-axis content must fit inside the
                    // container. This required inequality is what lets content
                    // size the container (hug/fittingSize) — without it, the last
                    // child overflows instead of widening/heightening it.
                    constraints.push(
                        bottom
                            .constraint_less_than_or_equal_to(&view.bottom)
                            .offset(-style.padding),
                    );
                    // the fill/hug equality: required for grow, optional
                    // otherwise (yields to intrinsic sizes, so slack sits at
                    // the end of a sized container like flexbox's default)
                    let pin = bottom
                        .constraint_equal_to(&view.bottom)
                        .offset(-style.padding);
                    constraints.push(match has_grow {
                        true => pin,
                        false => optional(pin),
                    });
                }
            }
            Direction::Row => {
                let mut prev_trailing: Option<LayoutAnchorX> = None;
                let mut grow_trailing: Option<LayoutAnchorX> = None;

                for (child_el, child_widget) in el.children.iter().zip(child_widgets) {
                    let child_style = FlexStyle::from_props(&child_el.props);
                    let a = child_widget.anchors();

                    // main axis: lay out left-to-right
                    constraints.push(match prev_trailing.take() {
                        None => a
                            .leading
                            .constraint_equal_to(&view.leading)
                            .offset(style.padding),
                        Some(prev) => a.leading.constraint_equal_to(&prev).offset(style.gap),
                    });
                    prev_trailing = Some(a.trailing.clone());

                    // cross axis: stretch to the container's height.
                    // NB: a Row needs a height from somewhere — a height prop,
                    // grow within a parent column, or the root. An unsized row
                    // is ambiguous.
                    constraints.push(a.top.constraint_equal_to(&view.top).offset(style.padding));
                    constraints.push(
                        a.bottom
                            .constraint_equal_to(&view.bottom)
                            .offset(-style.padding),
                    );

                    if let Some(h) = child_style.height {
                        constraints.push(a.height.constraint_equal_to_constant(h));
                    }
                    if let Some(w) = child_style.width {
                        constraints.push(a.width.constraint_equal_to_constant(w));
                    }
                    if child_style.grow {
                        grow_trailing = Some(a.trailing);
                    }
                }

                let has_grow = grow_trailing.is_some();
                if let Some(trailing) = grow_trailing.or(prev_trailing) {
                    // flexbox containment: main-axis content must fit inside
                    // the container (see Column branch for the rationale)
                    constraints.push(
                        trailing
                            .constraint_less_than_or_equal_to(&view.trailing)
                            .offset(-style.padding),
                    );
                    let pin = trailing
                        .constraint_equal_to(&view.trailing)
                        .offset(-style.padding);
                    constraints.push(match has_grow {
                        true => pin,
                        false => optional(pin),
                    });
                }
            }
        }

        constraints
    }

    /// Reconciliation: walks the old mounted widget tree and the new element
    /// tree together, REUSING widgets whose elements line up (the objc views,
    /// their text, selection, and focus all survive) and refreshing their
    /// props and handlers. Where they don't line up, the old subtree unmounts
    /// and a fresh one mounts in its place.
    fn patch(&self, parent: &View, widget: &mut Widget, old_el: &Element, new_el: &Element) {
        match (widget, &new_el.element_type) {
            (widget @ Widget::Container { .. }, ElementType::Window | ElementType::Div) => {
                self.patch_container(widget, old_el, new_el);
            }

            (
                Widget::Button {
                    control,
                    handler_id,
                },
                ElementType::Button,
            ) => {
                // reconcile the title
                if button_label(old_el) != button_label(new_el) {
                    let title = NSString::new(button_label(new_el).as_str());
                    control.objc.with_mut(|obj| unsafe {
                        let _: () = msg_send![obj, setTitle:&*title];
                    });
                }

                // refresh the handler under the same dispatch id — the
                // button's action closure keeps firing this id, so nothing
                // grows and the handler is always current
                if let Some(Handler::Simple(event)) = new_el.handlers.get("on_click") {
                    match handler_id {
                        Some(id) => {
                            self.handlers_by_id.borrow_mut().insert(*id, event.clone());
                        }
                        None => {
                            let id = self.next_handler_id.replace(self.next_handler_id.get() + 1);
                            self.handlers_by_id.borrow_mut().insert(id, event.clone());
                            let send = self.dispatch_event.clone();
                            control.set_action(move || {
                                send(id);
                            });
                            *handler_id = Some(id);
                        }
                    }
                }
            }

            (Widget::Input(field), ElementType::Input) => {
                // controlled value: write only what actually differs — a
                // reused field's cursor must never move
                if old_el.props.get("value") != new_el.props.get("value") {
                    if let Some(value) = new_el.props.get("value") {
                        if field.get_value() != *value {
                            field.set_text(value);
                        }
                    }
                }
                if old_el.props.get("placeholder") != new_el.props.get("placeholder") {
                    if let Some(placeholder) = new_el.props.get("placeholder") {
                        field.set_placeholder_text(placeholder);
                    }
                }
                // refresh on_change on the delegate
                if let Some(delegate) = field.delegate.as_ref() {
                    *delegate.on_change.borrow_mut() = new_el.handlers.get("on_change").cloned();
                }
            }

            (Widget::Label(label), ElementType::Text(text)) => {
                // display-only: no cursor to protect, just refresh
                label.set_text(text);
            }

            (Widget::ImageView { view, version }, ElementType::Image) => {
                // re-blit only when a new frame arrived — the version check
                // that makes idle renders free
                if let Some(src_key) = new_el.props.get("src") {
                    let frame = self.state.use_state::<BlitFrame>(src_key, BlitFrame::empty);
                    if frame.version != *version {
                        set_frame(view, &frame);
                        *version = frame.version;
                    }
                }
            }

            (Widget::List(control), ElementType::List) => {
                // Rows are render outputs: refresh the snapshot wholesale.
                // Deliberately NOT calling reload() here — it would fire
                // item_for mid-render while the state borrow is held; the
                // post-render reload_lists() pass does it with the borrow
                // released.
                let rows = self.snapshot_rows(new_el);
                if let Some(delegate) = control.delegate.as_ref() {
                    *delegate.rows.borrow_mut() = rows;
                }
            }

            // incompatible: unmount the old subtree (it drops, its views
            // remove themselves) and mount the new element fresh
            (widget, _) => {
                let mut fresh = self.mount_element(parent, new_el);
                self.layout_node(new_el, &mut fresh);
                *widget = fresh;
            }
        }
    }

    /// Container reconciliation: children match by position — same kind in
    /// the same slot is patched recursively; new children mount; vanished
    /// children drop; changed kinds replace. The container relayouts itself
    /// (deactivating its stored constraints, regenerating) when anything
    /// structural or flex-relevant changed.
    fn patch_container(&self, widget: &mut Widget, old_el: &Element, new_el: &Element) {
        let Widget::Container {
            view,
            children,
            constraints,
            ..
        } = widget
        else {
            unreachable!("patch_container called on a non-container")
        };

        // reconcile the background prop (visual only)
        if old_el.props.get("background") != new_el.props.get("background") {
            if let Some(bg) = new_el.props.get("background") {
                view.set_background_color(color(bg));
            }
        }

        let mut needs_relayout =
            old_el.children.len() != new_el.children.len() || flex_changed(old_el, new_el);

        for index in 0..new_el.children.len() {
            let new_child = &new_el.children[index];

            let mut slot_ok = false;
            if let (Some(child_widget), Some(old_child)) =
                (children.get_mut(index), old_el.children.get(index))
            {
                slot_ok = compatible(child_widget, new_child);
                if slot_ok {
                    self.patch(view, child_widget, old_child, new_child);
                    if flex_changed(old_child, new_child) {
                        needs_relayout = true;
                    }
                }
            }

            if !slot_ok {
                // replaced (old drops) or appended
                let mut fresh = self.mount_element(view, new_child);
                self.layout_node(new_child, &mut fresh);
                match children.get_mut(index) {
                    Some(slot) => *slot = fresh,
                    None => children.push(fresh),
                }
                needs_relayout = true;
            }
        }

        // vanished children drop — their views remove themselves
        children.truncate(new_el.children.len());

        if needs_relayout {
            LayoutConstraint::deactivate(constraints);
            let generated = self.container_constraints(new_el, view, children);
            LayoutConstraint::activate(&generated);
            *constraints = generated;
        }
    }
}

/// Messages that cross onto the main queue — the app's single Send
/// boundary. Widget events travel as dispatch ids; `App(M)` carries the
/// app's own messages (frames, progress, log lines...) from background
/// threads to the GUI. Anything a thread wants to say must fit in here
/// (the same rule as React Native's bridge).
///
/// `M` appears in exactly three places in the framework — this enum, the
/// delegate (`ReactApp<M>`), and `run` — because the button dispatch path
/// goes through an injected closure (`AppState::dispatch_event`) instead
/// of naming the concrete types.
pub enum Message<M> {
    /// a widget event fired — look up its handler by dispatch id
    Event(usize),
    /// an app message, from anywhere
    App(M),
}

pub struct ReactApp<M> {
    state: Rc<RefCell<AppState>>,
    /// the app's message handler: receives `App` messages with &State; the
    /// framework re-renders afterwards
    on_app_message: Rc<dyn Fn(&State, M)>,
}

impl<M: Send + Sync + 'static> ReactApp<M> {
    fn new(root: Box<dyn Component>, on_app_message: impl Fn(&State, M) + 'static) -> Self {
        // Render the tree once before the window exists: resizability can only
        // be set at creation, and a specified size should give the window its
        // initial dimensions. Components are pure, so rendering early is free
        // (slots initialized into this scratch State are simply re-initialized
        // with the same values on the real render).
        let scratch = State::default();
        let spec = window_spec(&root.render(&Ctx { state: &scratch }, vec![]));

        let mut config = WindowConfig::default();
        if !spec.resizable {
            // dialog-style: the default style set, minus Resizable
            config.set_styles(&[
                WindowStyle::Miniaturizable,
                WindowStyle::UnifiedTitleAndToolbar,
                WindowStyle::Closable,
                WindowStyle::Titled,
                WindowStyle::FullSizeContentView,
            ]);
        }
        if spec.width.is_some() || spec.height.is_some() {
            config.set_initial_dimensions(
                100.,
                100.,
                spec.width.unwrap_or(1024.),
                // No title-bar math here — the window doesn't exist yet, so
                // there's nothing to measure. The first render (before the
                // window is shown) measures the real inset and corrects.
                spec.height.unwrap_or(768.),
            );
        }
        let content = View::new();
        // the root sits below the title bar, so make the strip behind the
        // title bar blend with the window background
        content.set_background_color(Color::rgb(151, 143, 143));

        // The window delegate needs a way back into the shared app, but
        // AppState contains the window — so it holds a weak back-reference
        // cell, filled in just below.
        let weak_cell: Rc<RefCell<Option<Weak<RefCell<AppState>>>>> = Rc::new(RefCell::new(None));
        let proxy = WindowProxy {
            app: weak_cell.clone(),
            window: RefCell::new(None),
        };

        // The single place the App/delegate types are named: the injected
        // event dispatcher. Buttons capture this instead of the concrete
        // types, which is what keeps everything under AppState non-generic.
        let dispatch_event: Arc<dyn Fn(usize) + Send + Sync> = Arc::new(|id: usize| {
            App::<ReactApp<M>, Message<M>>::dispatch_main(Message::Event(id));
        });

        let state = Rc::new(RefCell::new(AppState {
            root_widget: None,
            state: State::default(),
            root,
            window: Window::with(config, proxy),
            content,
            app_weak: weak_cell.clone(),
            dispatch_event,
            last_tree: None,
            root_pins: RefCell::new(Vec::new()),
            handlers_by_id: RefCell::new(HashMap::new()),
            resize_handler: RefCell::new(None),
            next_handler_id: Cell::new(0),
            last_requested_size: Cell::new(None),
        }));
        *weak_cell.borrow_mut() = Some(Rc::downgrade(&state));

        Self {
            state,
            on_app_message: Rc::new(on_app_message),
        }
    }
}

impl<M> AppDelegate for ReactApp<M> {
    fn did_finish_launching(&self) {
        // Nib-less apps get no default menu bar: set the standard one (app
        // menu with Quit, File > Close, Window...) — cmd+q / cmd+w come from
        // the menu items' key equivalents.
        App::set_menu(Menu::standard());

        {
            let state = self.state.borrow();
            state.window.set_content_view(&state.content);
        }

        // render before show, so a content-hugging window is born at the right
        // size instead of resizing in view. The reload pass makes AppKit
        // re-query any lists after the render borrow is released (their
        // item_for may have been skipped mid-render).
        self.state.borrow_mut().render();
        self.state.borrow().reload_lists();

        self.state.borrow().window.show();

        // Kick off activation after the window is ordered front — when
        // launched from a terminal the app isn't the foreground process yet.
        // The window becoming *key* is finished off in did_become_active,
        // which fires once the activation handshake actually completes.
        // Note: this probably won't work to activate the app unless it's
        // packaged properly due to changes in MacOs.
        App::activate();
    }

    /// The reliable place to claim focus: called after the app is genuinely
    /// active, which may happen well after did_finish_launching (launching
    /// from a terminal, slow activation...). makeKeyAndOrderFront only makes
    /// a window key while the app is active, so this is where it sticks.
    fn did_become_active(&self) {
        self.state.borrow().window.make_key_and_order_front();
    }

    fn should_terminate_after_last_window_closed(&self) -> bool {
        true
    }
}

impl<M: Send + Sync + 'static> Dispatcher for ReactApp<M> {
    type Message = Message<M>;

    /// The React loop: messages land here on the main thread, via the main
    /// queue. Widget events fire their handler then re-render; app messages
    /// run the app's handler with &State, then re-render — the tree picks
    /// them up like any other state.
    fn on_ui_message(&self, message: Message<M>) {
        // Each arm: run the handler and render under the borrow, then RELEASE
        // the borrow before reload_lists — item_for must find it free.
        match message {
            Message::Event(id) => {
                let handler = self
                    .state
                    .borrow()
                    .handlers_by_id
                    .borrow()
                    .get(&id)
                    .cloned();
                match handler {
                    Some(handler) => {
                        {
                            let mut app = self.state.borrow_mut();
                            handler.fire(&app.state);
                            app.render();
                        }
                        self.state.borrow().reload_lists();
                    }
                    None => println!("warning: no event #{} — stale dispatch?", id),
                }
            }
            Message::App(msg) => {
                {
                    let mut app = self.state.borrow_mut();
                    (self.on_app_message)(&app.state, msg);
                    app.render();
                }
                self.state.borrow().reload_lists();
            }
        }
    }
}

/// Puts an app message onto the main queue. Call from any thread — the
/// app-side half of the Send boundary.
pub fn dispatch<M: Send + Sync + 'static>(message: M) {
    App::<ReactApp<M>, Message<M>>::dispatch_main(Message::App(message));
}

fn widget_at_path<'a>(widget: &'a Widget, path: &[usize]) -> Option<&'a Widget> {
    let mut current = widget;
    for index in path {
        let Widget::Container { children, .. } = current else {
            return None;
        };
        current = children.get(*index)?;
    }
    Some(current)
}

/// Walks the widget tree asking every list to reload (see AppState::reload_lists).
fn reload_lists_in(widget: &Widget) {
    match widget {
        Widget::List(control) => control.reload(),
        Widget::Container { children, .. } => {
            for child in children {
                reload_lists_in(child);
            }
        }
        _ => {}
    }
}

/// Finds the position of the focused input in the widget tree. While
/// editing, the first responder is actually the field's *editor*, whose
/// delegate is the field itself — so match either.
fn find_focused(
    widget: &Widget,
    first_responder: *mut Object,
    path: &mut Vec<usize>,
) -> Option<(Vec<usize>, *mut Object)> {
    match widget {
        Widget::Input(field) => {
            let field_ptr = field.objc.get(|obj| obj as *const Object as *mut Object);
            let editor_delegates_to_field = unsafe {
                let delegate: *mut Object = msg_send![first_responder, delegate];
                delegate == field_ptr
            };
            if first_responder == field_ptr || editor_delegates_to_field {
                return Some((path.clone(), field_ptr));
            }
        }
        Widget::Container { children, .. } => {
            for (index, child) in children.iter().enumerate() {
                path.push(index);
                if let Some(found) = find_focused(child, first_responder, path) {
                    return Some(found);
                }
                path.pop();
            }
        }
        _ => {}
    }
    None
}

/// Foundation's NSRange, for placing the insertion point after refocusing.
#[repr(C)]
struct NSRange {
    location: usize,
    length: usize,
}

fn color(name: &str) -> Color {
    match name {
        "blue" => Color::SystemBlue,
        "red" => Color::SystemRed,
        "green" => Color::SystemGreen,
        "gray" => Color::SystemGray,
        _ => Color::SystemBrown,
    }
}

/// The display text of a Button element: its first Text child, if any.
fn button_label(el: &Element) -> String {
    el.children
        .iter()
        .find_map(|child| match &child.element_type {
            ElementType::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Marks a constraint as optional (priority 250, below the ~251 priority of
/// intrinsic content sizes). Used for the "pin the last child to the far edge"
/// rule: when the container has room, the pin stretches the last child; when it
/// would fight a button's intrinsic size, the solver breaks the pin instead and
/// the slack sits at the end of the container — like flexbox's default.
fn optional(constraint: LayoutConstraint) -> LayoutConstraint {
    unsafe {
        // UILayoutPriority is a `float` (f32), not CGFloat — passing an f64
        // puts garbage in the register and AppKit throws an exception.
        let priority: f32 = 50.;
        let _: () = msg_send![&*constraint.constraint, setPriority: priority];
    }
    constraint
}

/// Debug helper: prints the solved frame of every widget in the tree. The
/// frames are only meaningful after a layout pass has run
/// (`layoutSubtreeIfNeeded` on the root container forces one).
#[cfg(feature = "debug_dump")]
fn debug_dump(el: &Element, widget: &Widget, depth: usize) {
    let indent = "  ".repeat(depth);
    let frame: cacao::core_graphics::display::CGRect = match widget {
        Widget::Container { view, .. } => view.objc.get(|obj| unsafe { msg_send![obj, frame] }),
        Widget::Button { control, .. } => control.objc.get(|obj| unsafe { msg_send![obj, frame] }),
        Widget::Label(label) => label.objc.get(|obj| unsafe { msg_send![obj, frame] }),
        Widget::Input(field) => field.objc.get(|obj| unsafe { msg_send![obj, frame] }),
        Widget::ImageView { view, .. } => view.objc.get(|obj| unsafe { msg_send![obj, frame] }),
        Widget::List(control) => control.objc.get(|obj| unsafe { msg_send![obj, frame] }),
    };
    println!(
        "{indent}{:?}: ({:.0}, {:.0}) {:.0} x {:.0}",
        el.element_type, frame.origin.x, frame.origin.y, frame.size.width, frame.size.height
    );

    if let Widget::Container { children, .. } = widget {
        for (child_el, child_widget) in el.children.iter().zip(children) {
            debug_dump(child_el, child_widget, depth + 1);
        }
    }
}

/// Blits raw RGBA bytes into an NSImageView via NSBitmapImageRep — the
/// classic AppKit path for raw pixel buffers (cacao's `Image` only takes
/// encoded file data). NSBitmapImageRep's data is top-down, matching the
/// tracer's row order.
fn set_frame(view: &ImageView, frame: &BlitFrame) {
    let (w, h) = (frame.width, frame.height);
    if w == 0 || h == 0 || frame.pixels.len() != w * h * 4 {
        return;
    }

    unsafe {
        let rep: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
        let space = NSString::new("NSDeviceRGBColorSpace");
        // Every numeric arg is explicitly isize: msg_send transmutes the
        // call, and with this many args some spill to the stack — a 32-bit
        // literal lands in a 64-bit slot with garbage in the upper half,
        // and AppKit rejects the "inconsistent" values. (Learned via probe.)
        let (px_w, px_h): (isize, isize) = (w as isize, h as isize);
        let (bps, spp): (isize, isize) = (8, 4);
        let (row, bpp): (isize, isize) = ((w * 4) as isize, 32);
        let rep: *mut Object = msg_send![rep,
            initWithBitmapDataPlanes:nil
            pixelsWide:px_w pixelsHigh:px_h
            bitsPerSample:bps samplesPerPixel:spp
            hasAlpha:true isPlanar:false
            colorSpaceName:&*space
            bytesPerRow:row bitsPerPixel:bpp];
        if rep.is_null() {
            println!("set_frame: rep init FAILED");
            return;
        }
        let dst: *mut u8 = msg_send![rep, bitmapData];
        std::ptr::copy_nonoverlapping(frame.pixels.as_ptr(), dst, frame.pixels.len());

        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithSize:CGSize::new(w as f64, h as f64)];
        let _: () = msg_send![image, addRepresentation:rep];

        view.objc.with_mut(|obj| {
            let _: () = msg_send![obj, setImage:image];
        });
    }
}

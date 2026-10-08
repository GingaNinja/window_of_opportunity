//! The task from "A 2025 Survey of Rust GUI Libraries"
//! (https://www.boringcactus.com/2025/04/13/2025-survey-of-rust-gui-libraries.html),
//! automated: a text label and an input field that share one piece of
//! state — type in the input and the label follows. The blog grades every
//! library on three checks, and all three are automatable on Windows:
//!
//!   1. works at all?  — typing in the input changes the label
//!   2. screen reader? — Narrator can read the label text AND the input's
//!      contents (the survey's "the field is there but its contents are
//!      invisible" failures are UIA value bugs)
//!   3. IME works?     — composed kanji (東京) lands in the field intact
//!
//! Each test boots the blog's app for real (real window, real EDIT/static
//! controls, real message loop on its own thread) and drives it with the
//! same messages the shell would send. The three checks map 1:1 to the
//! survey table's three columns.
#![cfg(target_os = "windows")]

use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
    UIA_EditControlTypeId, UIA_TextControlTypeId, UIA_ValuePatternId,
};
use windows::Win32::UI::Controls::EM_SETSEL;
use windows::Win32::UI::Input::Ime::{
    GCS_COMPSTR, ImmGetContext, ImmReleaseContext, ImmSetCompositionStringW, SCS_SETSTR,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, FindWindowW, GetClassNameW, GetWindowTextW, PostMessageW, SendMessageW,
    WM_CHAR, WM_CLOSE, WM_IME_CHAR, WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION,
    WM_IME_STARTCOMPOSITION,
};
use windows::core::{BOOL, PCWSTR, w};

use window_of_opportunity::{app::Application, component::Component, ui};

// ---------------------------------------------------------------------------
// The survey's app: label + input over one `text` slot, the React sample
// every library in the post had to reproduce. The window title is the
// test's handle on its window — unique per test so the suite can run in
// parallel without window lookups crossing wires.
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Survey {
    title: &'static str,
}

impl Component for Survey {
    fn render(
        &self,
        ctx: &window_of_opportunity::state::Ctx,
        _children: Vec<Box<window_of_opportunity::element::Element>>,
    ) -> Box<window_of_opportunity::element::Element> {
        // ui! props are token-expanded, so multi-token values need a
        // binding first (same as the examples' `title(title)` shape)
        let title = self.title;
        let text = ctx.use_state("text", || "Hello, world!".to_string());
        ui! {
            Window title(title) {
                { Div direction("column") gap(6.) padding(8.) {
                    { Text format!("{text}") }
                    { Input value(text) placeholder("type here...")
                            on_change(|state, t| state.update("text", |s: &mut String| *s = t)) }
                } }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Harness: boot the app on its own thread (run blocks on the message
// loop), find its controls, drive them the way the shell does, shut it
// down through the normal WM_CLOSE path.
// ---------------------------------------------------------------------------

struct SurveyApp {
    hwnd: HWND,
    thread: Option<thread::JoinHandle<()>>,
}

impl SurveyApp {
    fn boot(title: &'static str) -> SurveyApp {
        let thread = thread::spawn(move || {
            let app = Application {};
            app.run(Box::new(Survey { title }), |_state, _msg: ()| ());
        });
        let hwnd = wait_for_window(title);
        SurveyApp {
            hwnd,
            thread: Some(thread),
        }
    }

    /// The label: the survey's single `static` control.
    fn label(&self) -> HWND {
        find_control(self.hwnd, "static")
    }

    /// The input: the survey's single `EDIT` control.
    fn input(&self) -> HWND {
        find_control(self.hwnd, "EDIT")
    }

    /// Types `text` into the input the way a user would: select the old
    /// contents, then one WM_CHAR per character — exactly the messages
    /// TranslateMessage produces for keystrokes (and the channel an IME
    /// result arrives on). Each character fires EN_CHANGE, so this
    /// exercises the per-keystroke state round trip the survey cares
    /// about.
    fn type_text(&self, text: &str) {
        let edit = self.input();
        unsafe {
            // select-all: the first character replaces the old contents
            let _ = SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            for unit in text.encode_utf16() {
                let _ = SendMessageW(edit, WM_CHAR, Some(WPARAM(unit as usize)), Some(LPARAM(1)));
            }
        }
    }

    fn label_text(&self) -> String {
        window_text(self.label())
    }

    fn input_text(&self) -> String {
        window_text(self.input())
    }
}

impl Drop for SurveyApp {
    fn drop(&mut self) {
        // the ordinary close path — WM_CLOSE → DestroyWindow → WM_DESTROY
        // → PostQuitMessage — so the message loop ends and the thread
        // joins even when an assertion panics mid-test
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Polls for the window — creation happens on the app thread, so the
/// title may not resolve on the first lookup.
fn wait_for_window(title: &str) -> HWND {
    let wide = to_wide(title);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(hwnd) = unsafe { FindWindowW(w!("wo_mainwin"), PCWSTR(wide.as_ptr())) } {
            return hwnd;
        }
        assert!(Instant::now() < deadline, "window {title:?} never appeared");
        thread::sleep(Duration::from_millis(10));
    }
}

/// First descendant window of `root` whose class name matches — the label
/// and input live inside a Div, so a direct-children search isn't enough.
/// Polled: the window exists from CreateWindowExW onward, but its
/// controls mount in the first render, a beat later.
fn find_control(root: HWND, class: &str) -> HWND {
    struct Search {
        class: String,
        found: Option<HWND>,
    }
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        let mut buf = [0u16; 64];
        let n = unsafe { GetClassNameW(hwnd, &mut buf) };
        let name = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        if name.eq_ignore_ascii_case(&search.class) {
            search.found = Some(hwnd);
            BOOL(0) // stop
        } else {
            BOOL(1)
        }
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let mut search = Search {
            class: class.to_string(),
            found: None,
        };
        unsafe {
            let _ = EnumChildWindows(
                Some(root),
                Some(enum_proc),
                LPARAM(&mut search as *mut Search as isize),
            );
        }
        if let Some(found) = search.found {
            return found;
        }
        assert!(
            Instant::now() < deadline,
            "no {class} control in the survey window"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Polls `check` until true — the assertions here read real controls, and
/// a re-render can be a few messages behind the one that triggered it.
fn eventually<F: FnMut() -> bool>(what: &str, mut check: F) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

// ---------------------------------------------------------------------------
// Check 1: "works at all?" — the survey's task, the React sample's
// behaviour: label and input mirror one state, typing changes both.
// ---------------------------------------------------------------------------

#[test]
fn works_at_all_typing_in_the_input_changes_the_label() {
    let app = SurveyApp::boot("survey: works");

    // the shared initial state — the blog's "Hello, world!" screenshot
    assert_eq!(app.label_text(), "Hello, world!");
    assert_eq!(app.input_text(), "Hello, world!");

    // typing replaces it in both places
    app.type_text("Hello, tests!");
    eventually("the label to follow the input", || {
        app.label_text() == "Hello, tests!"
    });
    assert_eq!(app.input_text(), "Hello, tests!");

    // and the second edit works as well as the first (no one-shot state)
    app.type_text("Hello, again!");
    eventually("the label to follow again", || {
        app.label_text() == "Hello, again!"
    });
    assert_eq!(app.input_text(), "Hello, again!");
}

// ---------------------------------------------------------------------------
// Check 2: "screen reader accessible?" — Narrator reads UIA: the label's
// text is its Name, the input's contents its Value pattern. The survey
// failed libraries whose text fields hid their contents from Narrator
// (and those that hid the text entirely), so both halves are asserted.
// ---------------------------------------------------------------------------

#[test]
fn screen_reader_narrator_reads_the_label_and_the_input_contents() {
    let app = SurveyApp::boot("survey: narrator");
    app.type_text("Tokyo 東京");
    eventually("the label to show the typed text", || {
        app.label_text() == "Tokyo 東京"
    });

    let (label, input) = uia_texts(app.hwnd);
    assert_eq!(
        label.as_deref(),
        Some("Tokyo 東京"),
        "Narrator can't read the label text"
    );
    assert_eq!(
        input.as_deref(),
        Some("Tokyo 東京"),
        "Narrator can't read the input's contents"
    );
}

/// What Narrator sees: the text element's Name (the label's words) and the
/// edit element's Value (the input's contents), via the same UIA core
/// Narrator drives.
fn uia_texts(window: HWND) -> (Option<String>, Option<String>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let uia: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).expect("CUIAutomation");
        let root = uia.ElementFromHandle(window).expect("element for window");
        let cond = uia.CreateTrueCondition().expect("true condition");
        let all = root
            .FindAll(TreeScope_Descendants, &cond)
            .expect("descendants");
        let mut label = None;
        let mut input = None;
        for i in 0..all.Length().expect("descendant count") {
            let el = all.GetElement(i).expect("descendant");
            let control_type = el.CurrentControlType().expect("control type");
            if control_type == UIA_TextControlTypeId {
                label = Some(el.CurrentName().expect("label name").to_string());
            } else if control_type == UIA_EditControlTypeId {
                let value: IUIAutomationValuePattern = el
                    .GetCurrentPatternAs(UIA_ValuePatternId)
                    .expect("edit value pattern");
                input = Some(value.CurrentValue().expect("input value").to_string());
            }
        }
        (label, input)
    }
}

// ---------------------------------------------------------------------------
// Check 3: "IME works?" — the survey types toukyou, converts to 東京. Two
// things must hold: the composer's provisional text never reaches the
// state (the flutter_rust_bridge failure mode), and the converter's kanji
// lands intact in the input and echoes to the label. The composition is
// driven through the real IMM32 messages — the same pipeline a Japanese
// IME uses — not through plain keystrokes.
// ---------------------------------------------------------------------------

#[test]
fn ime_composed_kanji_lands_intact() {
    let app = SurveyApp::boot("survey: ime");
    let edit = app.input();

    ime_compose(edit, "とうきょう", "東京");

    eventually("the committed kanji to reach the input", || {
        app.input_text() == "東京"
    });
    eventually("the committed kanji to reach the label", || {
        app.label_text() == "東京"
    });
    // and the provisional hiragana must NOT have leaked into the state —
    // exactly once, no composer leftovers
    assert_eq!(app.input_text(), "東京");
    assert_eq!(app.label_text(), "東京");
}

/// Runs the composition protocol against the edit control the way the
/// Windows IME does: the composer phase (toukyou → とうきょう) opens a
/// composition, and the converter commits its result — delivered as
/// WM_IME_CHAR units, the message the IME uses to hand converted text to
/// the control. The control — and behind it the app's on_change → state
/// → label pipeline — sees the IME message path, not plain keystrokes.
///
/// The composer's provisional display is best-effort here: storing the
/// provisional string needs an IME attached to the window's thread, so
/// when that call fails (no Japanese IME on this machine) the converter
/// path alone is exercised. Provisional *pixels* are the OS control's
/// job in any case — the blog checked those by eye.
fn ime_compose(edit: HWND, provisional: &str, committed: &str) {
    unsafe {
        // select-all: the composition replaces the old contents, the
        // same starting point type_text gives its keystrokes
        let _ = SendMessageW(edit, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
        SendMessageW(
            edit,
            WM_IME_STARTCOMPOSITION,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );

        // composer: provisional text — best-effort, see above
        let himc = ImmGetContext(edit);
        if !himc.0.is_null() {
            let comp = to_wide(provisional);
            let ok = ImmSetCompositionStringW(
                himc,
                SCS_SETSTR,
                Some(comp.as_ptr() as *const _),
                ((comp.len() - 1) * 2) as u32, // bytes, without the terminator
                None,
                0,
            );
            if ok.as_bool() {
                SendMessageW(
                    edit,
                    WM_IME_COMPOSITION,
                    Some(WPARAM(0)),
                    Some(LPARAM(GCS_COMPSTR.0 as isize)),
                );
            }
            let _ = ImmReleaseContext(edit, himc);
        }

        // converter: the confirmed kanji, one WM_IME_CHAR per UTF-16 unit
        for unit in committed.encode_utf16() {
            SendMessageW(
                edit,
                WM_IME_CHAR,
                Some(WPARAM(unit as usize)),
                Some(LPARAM(1)),
            );
        }

        SendMessageW(
            edit,
            WM_IME_ENDCOMPOSITION,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );
    }
}

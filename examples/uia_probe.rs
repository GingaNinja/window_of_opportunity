//! Narrator's view of any window — the UIA tree the screen reader
//! consumes, printed live, with NO keyboard chords required.
//!
//! The manual check "can Narrator read the label?" really asks "is the
//! label's text in the UIA tree, under the right name?" Narrator is just
//! a UIA client — and so is this probe. If the label's words show up
//! below, Narrator has everything it needs to speak them; the only thing
//! left to verify by ear is the audio itself.
//!
//!     cargo run --example survey        # the app under test
//!     cargo run --example uia_probe     # this: "Narrator's eye view"
//!
//! Type in the survey window and watch the tree change — the Text
//! element's Name and the Edit element's Value are exactly what Narrator
//! would say. An empty Name means nothing to speak there.
//!
//! Optional argument: a substring of the target window's title
//! (default "survey"), so the probe can inspect any window:
//!
//!     cargo run --example uia_probe -- notepad
#![cfg(target_os = "windows")]

use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
    UIA_EditControlTypeId, UIA_PaneControlTypeId, UIA_TextControlTypeId, UIA_ValuePatternId,
    UIA_WindowControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, IsWindow};
use windows::core::BOOL;

fn main() {
    let needle = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "survey".to_string())
        .to_lowercase();

    println!("looking for a window whose title contains {needle:?}");
    println!("(start `cargo run --example survey` first)\n");

    let Some(hwnd) = wait_for_window(&needle) else {
        println!("no matching window — start one, e.g.:");
        println!("    cargo run --example survey");
        return;
    };

    // one automation object for the whole poll loop
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let uia: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .expect("CUIAutomation");

    println!("probing — type in the target window; the tree reprints on change:\n");
    let mut last = String::new();
    loop {
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            println!("window closed");
            return;
        }
        let tree = uia_tree(&uia, hwnd);
        if tree != last {
            println!("---\n{tree}");
            last = tree;
        }
        thread::sleep(Duration::from_millis(300));
    }
}

/// The UIA tree as Narrator sees it: every descendant's control type and
/// Name (what a screen reader speaks), plus the Edit's Value (the
/// "contents" of a text field — the survey's most commonly hidden bit).
fn uia_tree(uia: &IUIAutomation, window: HWND) -> String {
    unsafe {
        let root = uia.ElementFromHandle(window).expect("element for window");
        let cond = uia.CreateTrueCondition().expect("true condition");
        let all = root
            .FindAll(TreeScope_Descendants, &cond)
            .expect("descendants");

        let mut out = String::new();
        out.push_str(&describe(root));
        for i in 0..all.Length().expect("descendant count") {
            let el = all.GetElement(i).expect("descendant");
            out.push_str("  ");
            out.push_str(&describe(el));
        }
        out
    }
}

/// One line per element — the fields a screen reader uses.
unsafe fn describe(el: windows::Win32::UI::Accessibility::IUIAutomationElement) -> String {
    unsafe {
        let ct = el.CurrentControlType().expect("control type");
        let kind = if ct == UIA_WindowControlTypeId {
            "Window"
        } else if ct == UIA_PaneControlTypeId {
            "Pane"
        } else if ct == UIA_TextControlTypeId {
            "Text  "
        } else if ct == UIA_EditControlTypeId {
            "Edit  "
        } else {
            "Other "
        };
        let name = el.CurrentName().unwrap_or_default().to_string();
        let mut line = format!("{kind} name={name:?}");
        if ct == UIA_EditControlTypeId {
            if let Ok(value) =
                el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
            {
                line.push_str(&format!(
                    " value={:?}",
                    value.CurrentValue().unwrap_or_default().to_string()
                ));
            }
        }
        line
    }
}

/// First top-level window whose title contains `needle` (case-insensitive).
fn wait_for_window(needle: &str) -> Option<HWND> {
    struct Found {
        needle: String,
        hwnd: Option<HWND>,
    }
    unsafe extern "system" fn proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Found) };
        let mut buf = [0u16; 256];
        let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        if title.to_lowercase().contains(&found.needle) {
            found.hwnd = Some(hwnd);
            BOOL(0) // stop
        } else {
            BOOL(1)
        }
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut found = Found {
            needle: needle.to_string(),
            hwnd: None,
        };
        unsafe {
            let _ = EnumWindows(Some(proc), LPARAM(&mut found as *mut Found as isize));
        }
        if let Some(hwnd) = found.hwnd {
            return Some(hwnd);
        }
        if Instant::now() > deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

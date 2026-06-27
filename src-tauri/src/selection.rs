//! Capture the text the user currently has selected in *any* application.
//!
//! There is no cross-platform API to read another app's selection directly, so
//! we do what every "fix my selection" tool does: synthesize the Copy shortcut
//! and read the clipboard. To stay polite to clipboard managers we borrow the
//! clipboard for a fraction of a second and put the user's original content
//! back. The corrected text never auto-pastes; the user copies it from the chat
//! when they want it, so the clipboard is only touched here, during capture.
//!
//! How we tell "nothing was selected" from "selected empty-ish text": we seed
//! the clipboard with a sentinel before sending Copy and poll until it changes.
//! If it never changes, the Copy was a no-op → nothing selected.

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

// Unlikely to ever equal a real selection (leading NUL). If Copy replaces it,
// something was selected.
const SENTINEL: &str = "\u{0}quill::no-selection";

// macOS copies with ⌘C, Windows/Linux with Ctrl+C.
#[cfg(target_os = "macos")]
const COPY_MODIFIER: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const COPY_MODIFIER: Key = Key::Control;

// The "C" of the copy chord.
//
// On macOS we send the *raw* keycode of the physical C key (kVK_ANSI_C = 0x08),
// NOT `Key::Unicode('c')`. `Key::Unicode` makes enigo resolve the keycode through
// the macOS Text Input Source APIs (TSM/HIToolbox), which assert they run on the
// main thread and hard-crash the process (SIGTRAP) when called from our worker
// thread — which is exactly where capture() runs. The raw keycode skips that
// lookup entirely, and as a bonus ⌘C then fires regardless of the active layout
// (e.g. a Cyrillic layout), which is what we want for a bilingual tool.
#[cfg(target_os = "macos")]
const COPY_KEY: Key = Key::Other(0x08);
#[cfg(not(target_os = "macos"))]
const COPY_KEY: Key = Key::Unicode('c');

/// Grab the current selection. Returns the trimmed selected text, or an empty
/// string when nothing is selected. The user's prior clipboard is restored
/// before returning.
pub fn capture() -> Result<String, String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("clipboard open: {}", e))?;

    // Remember what the user had. Text-only: restoring images/files is out of
    // scope, and we only borrow the clipboard for well under a second.
    let saved = clipboard.get_text().ok();

    clipboard
        .set_text(SENTINEL.to_string())
        .map_err(|e| format!("clipboard seed: {}", e))?;

    send_copy()?;

    // Poll: the OS delivers the synthetic Copy and the target app writes the
    // pasteboard asynchronously. ~1s ceiling is plenty even for sluggish apps.
    let mut captured = String::new();
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        if let Ok(current) = clipboard.get_text() {
            if current != SENTINEL {
                captured = current;
                break;
            }
        }
    }

    restore(&mut clipboard, saved);

    Ok(captured.trim().to_string())
}

fn send_copy() -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo
        .key(COPY_MODIFIER, Direction::Press)
        .map_err(|e| format!("modifier press: {}", e))?;
    // Let ⌘ register before C lands, and hold the chord a beat before releasing.
    // Without these the OS can see a bare "c" (copy never fires) — the difference
    // between a 0-char capture and a real one in fussy apps (terminals, Electron).
    std::thread::sleep(std::time::Duration::from_millis(20));
    enigo
        .key(COPY_KEY, Direction::Click)
        .map_err(|e| format!("c click: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    enigo
        .key(COPY_MODIFIER, Direction::Release)
        .map_err(|e| format!("modifier release: {}", e))?;
    Ok(())
}

fn restore(clipboard: &mut Clipboard, saved: Option<String>) {
    match saved {
        Some(text) => {
            let _ = clipboard.set_text(text);
        }
        // Nothing readable was there before (empty or non-text). Clearing is
        // the closest honest restore — leaving our sentinel behind would be worse.
        None => {
            let _ = clipboard.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression guard for a real crash: on macOS, routing the copy key through
    // `Key::Unicode` triggers a main-thread-only Text Input Source lookup and
    // SIGTRAPs the process from our worker thread. The copy key MUST stay a raw
    // keycode there. (No test on other platforms — Unicode is fine off-main.)
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_copy_key_is_raw_keycode_not_unicode() {
        assert!(
            matches!(COPY_KEY, Key::Other(_)),
            "macOS copy key must be a raw keycode, not Key::Unicode (off-main TSM crash)"
        );
    }
}

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

// Unlikely to ever equal a real selection (leading NUL). If Copy replaces it,
// something was selected.
const SENTINEL: &str = "\u{0}quill::no-selection";

// kVK_ANSI_C — the virtual keycode of the physical C key. We post ⌘C as a raw
// CGEvent with the Command flag set on the key event itself (see send_copy), so
// it fires regardless of the active layout (e.g. a Cyrillic layout) and the
// receiving app reads a real ⌘C from the event's own flags.
#[cfg(target_os = "macos")]
const KEY_C: u16 = 0x08;

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

// macOS: post ⌘C as a raw CGEvent with the Command flag set directly on the C
// key event.
//
// The previous approach (enigo: press ⌘, click C, release ⌘) left the Command
// flag *off* the C key event in many apps — terminals (Ghostty, Terminal),
// Electron — so the app saw a bare "c", the copy never fired, and capture()
// returned 0 chars every single time. Setting `CGEventFlagCommand` on the key
// event itself is the synthesis every selection-grabbing tool relies on: the
// receiving app reads the flag from the event, so it's a real ⌘C no matter what
// modifiers are physically held or which keyboard layout is active.
#[cfg(target_os = "macos")]
fn send_copy() -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| "CGEventSource::new failed".to_string())?;

    let down = CGEvent::new_keyboard_event(source.clone(), KEY_C, true)
        .map_err(|_| "copy key-down event".to_string())?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);

    // A short hold so apps that debounce keypresses still register the chord.
    std::thread::sleep(std::time::Duration::from_millis(15));

    let up = CGEvent::new_keyboard_event(source, KEY_C, false)
        .map_err(|_| "copy key-up event".to_string())?;
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    up.post(CGEventTapLocation::HID);

    Ok(())
}

// Windows/Linux: Ctrl+C via enigo. The macOS-only TSM crash that forced a raw
// keycode there doesn't apply, so the portable Unicode 'c' is fine here.
#[cfg(not(target_os = "macos"))]
fn send_copy() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| format!("modifier press: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    enigo
        .key(Key::Unicode('c'), Direction::Click)
        .map_err(|e| format!("c click: {}", e))?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    enigo
        .key(Key::Control, Direction::Release)
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
    // The copy keycode must stay kVK_ANSI_C. If this drifts, ⌘C turns into
    // ⌘<some-other-key> and capture silently breaks.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_copy_keycode_is_c() {
        assert_eq!(super::KEY_C, 0x08, "kVK_ANSI_C must be 0x08");
    }
}

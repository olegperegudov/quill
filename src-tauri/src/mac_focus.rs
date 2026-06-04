//! Remember which application was frontmost when the hotkey fired, and bring it
//! back to the front before we type the corrected text into it.
//!
//! The old flow never showed a window, so the target app kept focus the whole
//! time and typing the result back was trivial. The editor window *does* take
//! focus when shown, so by the time the user hits "Apply" the target app is no
//! longer frontmost. We grab its process id at capture time (while it's still
//! frontmost) and re-activate it just before typing.
//!
//! No-op on non-macOS: hiding our window already returns focus to the
//! previously active window there, which is enough for the type-back.

#[cfg(target_os = "macos")]
pub fn remember_frontmost() -> Option<i32> {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace == nil {
            return None;
        }
        let app: id = msg_send![workspace, frontmostApplication];
        if app == nil {
            return None;
        }
        let pid: i32 = msg_send![app, processIdentifier];
        if pid <= 0 { None } else { Some(pid) }
    }
}

#[cfg(target_os = "macos")]
pub fn activate(pid: i32) -> Result<(), String> {
    use cocoa::base::{id, nil, BOOL};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let app: id = msg_send![
            class!(NSRunningApplication),
            runningApplicationWithProcessIdentifier: pid
        ];
        if app == nil {
            return Err(format!("app pid {} no longer running", pid));
        }
        // NSApplicationActivateIgnoringOtherApps = 1 << 1. Deprecated in macOS 14
        // but still the most reliable cross-version way to pull an app forward.
        let opts: u64 = 1 << 1;
        let _: BOOL = msg_send![app, activateWithOptions: opts];
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn remember_frontmost() -> Option<i32> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn activate(_pid: i32) -> Result<(), String> {
    Ok(())
}

//! macOS-specific window tweaks.
//!
//! Tauri 2 with `decorations: false, transparent: true` produces a square
//! NSWindow; CSS `border-radius` clips only the DOM contents, so the corners
//! of the window itself show the desktop wallpaper through the transparent
//! gaps. The fix is to round the NSWindow's content layer directly via
//! AppKit — same effect macOS gives every standard titled window.
//!
//! No-op on every other platform (Windows 11 DWM already rounds borderless
//! windows itself; Linux is out of scope for now).

#[cfg(target_os = "macos")]
pub fn apply_rounded_corners(window: &tauri::WebviewWindow, radius: f64) -> Result<(), String> {
    use cocoa::base::{id, nil, YES};
    use objc::{msg_send, sel, sel_impl};

    let ns_window = window.ns_window().map_err(|e| e.to_string())? as id;
    unsafe {
        // contentView is the WKWebView. Its CALayer won't clip the window
        // outline by itself, so we apply cornerRadius/masksToBounds to its
        // *superview* — the private _NSThemeFrame that actually paints the
        // window edge. This is the same trick used by most macOS Electron/
        // Tauri apps that need round corners on a borderless window.
        let content_view: id = msg_send![ns_window, contentView];
        if content_view == nil {
            return Err("contentView is nil".into());
        }
        let frame_view: id = msg_send![content_view, superview];
        if frame_view == nil {
            return Err("frame view is nil".into());
        }
        let _: () = msg_send![frame_view, setWantsLayer: YES];
        let layer: id = msg_send![frame_view, layer];
        if layer == nil {
            return Err("frame view layer is nil".into());
        }
        let r: f64 = radius;
        let _: () = msg_send![layer, setCornerRadius: r];
        let _: () = msg_send![layer, setMasksToBounds: YES];
        crate::debug_log::log(&format!("rounded corners applied: radius={}", r));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn apply_rounded_corners(_window: &tauri::WebviewWindow, _radius: f64) -> Result<(), String> {
    Ok(())
}

/// Make the window follow the user's current Space instead of being pinned to
/// the Space it was first shown on. Without this, clicking the tray icon while
/// on a different Space teleports the user back to the window's home Space.
///
/// NSWindowCollectionBehaviorMoveToActiveSpace = 1 << 1 = 2.
#[cfg(target_os = "macos")]
pub fn apply_spaces_behavior(window: &tauri::WebviewWindow) -> Result<(), String> {
    use cocoa::base::id;
    use objc::{msg_send, sel, sel_impl};

    let ns_window = window.ns_window().map_err(|e| e.to_string())? as id;
    unsafe {
        let behavior: u64 = 1 << 1;
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        crate::debug_log::log("collectionBehavior=MoveToActiveSpace applied");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn apply_spaces_behavior(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Move the window so it's centred on the mouse cursor, clamped to stay fully on
/// whichever screen the cursor is on. The hotkey pops the chat where you're
/// already looking instead of on some other Space (combined with
/// `apply_spaces_behavior`, which brings it to the active Space).
///
/// Must run on the main thread — it talks to AppKit directly. All coordinates
/// are Cocoa screen points: bottom-left origin, y pointing up, same convention
/// as `NSEvent.mouseLocation`, so no flipping is needed.
#[cfg(target_os = "macos")]
pub fn position_at_cursor(window: &tauri::WebviewWindow) -> Result<(), String> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSPoint, NSRect};
    use objc::{class, msg_send, sel, sel_impl};

    let ns_window = window.ns_window().map_err(|e| e.to_string())? as id;
    unsafe {
        let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];

        // The usable area (minus menu bar/Dock) of the screen under the cursor.
        // Fall back to the main screen if the cursor isn't inside any frame.
        let mut visible: NSRect = {
            let main: id = msg_send![class!(NSScreen), mainScreen];
            if main == nil {
                return Err("no main screen".into());
            }
            msg_send![main, visibleFrame]
        };
        let screens: id = msg_send![class!(NSScreen), screens];
        if screens != nil {
            let count: usize = msg_send![screens, count];
            for i in 0..count {
                let scr: id = msg_send![screens, objectAtIndex: i];
                let f: NSRect = msg_send![scr, frame];
                if mouse.x >= f.origin.x
                    && mouse.x <= f.origin.x + f.size.width
                    && mouse.y >= f.origin.y
                    && mouse.y <= f.origin.y + f.size.height
                {
                    visible = msg_send![scr, visibleFrame];
                    break;
                }
            }
        }

        let frame: NSRect = msg_send![ns_window, frame];
        let (w, h) = (frame.size.width, frame.size.height);

        // Centre on the cursor, then pull back inside the visible area so the
        // whole window is reachable (titlebar never under the menu bar). Hand-
        // rolled clamp so a window larger than the screen can't panic clamp().
        let clamp = |v: f64, lo: f64, hi: f64| if hi < lo { lo } else { v.max(lo).min(hi) };
        let x = clamp(mouse.x - w / 2.0, visible.origin.x, visible.origin.x + visible.size.width - w);
        let y = clamp(mouse.y - h / 2.0, visible.origin.y, visible.origin.y + visible.size.height - h);

        let _: () = msg_send![ns_window, setFrameOrigin: NSPoint::new(x, y)];
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn position_at_cursor(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

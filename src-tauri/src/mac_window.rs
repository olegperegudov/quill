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

// The chat window is turned into a non-activating NSPanel, the same mechanism
// Ribbit uses and the one Spotlight/Raycast are built on. Two things a plain
// NSWindow cannot do, and Quill needs both:
//
// * Appear on the Space the user is on *right now* without activating the app.
//   Activating teleports the user to the window's home Space — the bug behind
//   "the hotkey moves me to another desktop".
// * Leave the other Spaces alone. The workaround that used to buy the first
//   point was CanJoinAllSpaces, which gives the window no home Space by making
//   it resident on *every* Space: swipe to the next desktop and the chat was
//   already sitting there, only to vanish when the swipe finished. The panel
//   takes MoveToActiveSpace instead — it follows the user when summoned and
//   stays behind on the old desktop when they swipe away, which is what the
//   frog does and what Quill is supposed to do.
//
// The tauri_panel! macro expansion calls `.app_handle()`, which needs Manager.
#[cfg(target_os = "macos")]
use tauri::Manager as _;

#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(QuillPanel {
        config: {
            can_become_key_window: true,   // non-activating but still keyable → typing works
            can_become_main_window: false,
            is_floating_panel: false       // ordinary level; show_and_make_key brings it up
        }
    })

    panel_event!(QuillPanelEvents {
        window_did_resign_key(notification: &NSNotification) -> ()
    })
}

/// Convert the chat window into the panel and configure it. Called once at
/// setup, after the accessory activation policy is set.
#[cfg(target_os = "macos")]
pub fn setup_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
    use tauri_nspanel::{CollectionBehavior, StyleMask, WebviewWindowExt};

    let panel = window.to_panel::<QuillPanel>().map_err(|e| e.to_string())?;
    // Borderless (decorations are off) and non-activating, but resizable: the
    // desk is a window you size once, and an empty mask would nail it shut.
    panel.set_style_mask(StyleMask::empty().nonactivating_panel().resizable().into());
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .full_screen_auxiliary()
            .move_to_active_space()
            .into(),
    );
    // A utility panel hides itself when the app deactivates by default; the chat
    // stays put until focus actually leaves it.
    panel.set_hides_on_deactivate(false);

    // Focus goes elsewhere → back to the tray, one click from returning. The
    // chat hangs off the tray icon like that icon's own menu, and menus close
    // when you look away. Hiding, not ordering back: over a full-screen app
    // there is no "behind", and `orderBack:` orders a window *in*.
    let app = window.app_handle().clone();
    let handler = QuillPanelEvents::new();
    handler.window_did_resign_key(move |_notification| {
        crate::note_auto_hide();
        hide_panel(&app);
    });
    panel.set_event_handler(Some(handler.as_ref()));
    // AppKit only weakly references the delegate; this one lives for the whole
    // process, so hand its ownership over deliberately.
    std::mem::forget(handler);

    crate::debug_log::log("panel: chat window converted to non-activating NSPanel");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn setup_panel(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Whether the chat panel is currently on screen.
#[cfg(target_os = "macos")]
pub fn panel_visible(app: &tauri::AppHandle) -> bool {
    use tauri_nspanel::ManagerExt;
    app.get_webview_panel("editor").map(|p| p.is_visible()).unwrap_or(false)
}

/// Show the panel on the user's CURRENT Space (over full-screen apps included)
/// and give it keyboard focus, without activating Quill.
#[cfg(target_os = "macos")]
pub fn show_panel(app: &tauri::AppHandle) {
    use tauri_nspanel::ManagerExt;
    match app.get_webview_panel("editor") {
        Ok(p) => p.show_and_make_key(),
        Err(e) => crate::debug_log::log(&format!("show_panel: panel not found ({:?})", e)),
    }
}

/// Hide the panel to the tray.
#[cfg(target_os = "macos")]
pub fn hide_panel(app: &tauri::AppHandle) {
    use tauri_nspanel::ManagerExt;
    if let Ok(p) = app.get_webview_panel("editor") {
        p.hide();
    }
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

/// Bundle id of the app that's frontmost right now (e.g. `com.mitchellh.ghostty`).
/// Logged just before a capture so a `captured 0 chars` is traceable to the app
/// it targeted — different apps copy differently, and this says which one.
#[cfg(target_os = "macos")]
pub fn frontmost_app() -> String {
    use cocoa::base::{id, nil};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let ws: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        if ws == nil {
            return "unknown".into();
        }
        let app: id = msg_send![ws, frontmostApplication];
        if app == nil {
            return "unknown".into();
        }
        let bid: id = msg_send![app, bundleIdentifier];
        if bid == nil {
            return "unknown".into();
        }
        let cstr: *const std::os::raw::c_char = msg_send![bid, UTF8String];
        if cstr.is_null() {
            return "unknown".into();
        }
        std::ffi::CStr::from_ptr(cstr).to_string_lossy().into_owned()
    }
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_app() -> String {
    "n/a".into()
}

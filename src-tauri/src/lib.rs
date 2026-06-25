//! Quill — polish your writing in place.
//!
//! Select text in any app, press the global hotkey, and Quill sends the
//! selection to an LLM that fixes spelling/punctuation/grammar (RU + EN,
//! without changing meaning or tone) and types the corrected text back over
//! the selection.
//!
//! Where the pieces live:
//! - selection.rs — grab the current selection (synthetic Copy + clipboard)
//! - corrector.rs — call the LLM, return corrected text
//! - logger.rs    — local history of corrections (original → corrected)
//! - secrets.rs   — API key in the OS keychain
//!
//! The hotkey opens a chat window at the cursor showing your text and its
//! correction; you click a bubble to copy the result and paste it yourself —
//! Quill no longer types over the selection (which needed extra Accessibility
//! reach and broke on every update). Capture still uses a synthetic Copy.
//!
//! Forked from Ribbit (voice-to-text); the tray/updater/window/TCC plumbing is
//! shared, the audio pipeline is replaced by the selection→correct→chat flow.

mod accessibility;
mod corrector;
mod debug_log;
mod logger;
mod mac_window;
mod secrets;
mod selection;
mod tcc_reset;

use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, Emitter, Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_updater::UpdaterExt;

const BUNDLE_ID: &str = "com.quill.app";
const DEFAULT_SHORTCUT: &str = "ctrl+alt+e";

struct AppState {
    /// True while a capture is in flight — guards against the hotkey re-firing
    /// (key repeat, double-tap) before the previous run finishes.
    busy: bool,
    current_shortcut: String,
}

#[tauri::command]
fn get_config() -> Result<serde_json::Value, String> {
    let cfg = read_config();
    let provider_name = cfg["llm_provider"]
        .as_str()
        .unwrap_or(corrector::DEFAULT_PROVIDER)
        .to_string();

    // Preview of the active provider's key, if it happens to be in the process
    // env (it is once loaded from the keychain at startup).
    let active = corrector::find_provider(&provider_name)
        .unwrap_or_else(|| corrector::find_provider(corrector::DEFAULT_PROVIDER).unwrap());
    let key = std::env::var(active.env_var).unwrap_or_default();
    let preview = if key.len() > 8 {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else if !key.is_empty() {
        "****".to_string()
    } else {
        String::new()
    };

    // Per-provider "has a key" so the UI can show a saved chip on each.
    let mut provider_keys = serde_json::Map::new();
    for p in corrector::PROVIDERS {
        provider_keys.insert(p.name.into(), serde_json::Value::Bool(secrets::has_key(p.env_var)));
    }

    Ok(serde_json::json!({
        "has_api_key": secrets::has_key(active.env_var),
        "api_key_preview": preview,
        "llm_provider": provider_name,
        "llm_provider_keys": provider_keys,
        "history_days": history_days(),
    }))
}

#[tauri::command]
fn set_api_key(key: String, provider: Option<String>) -> Result<(), String> {
    let provider_name = provider
        .or_else(|| read_config()["llm_provider"].as_str().map(String::from))
        .unwrap_or_else(|| corrector::DEFAULT_PROVIDER.to_string());
    let p = corrector::find_provider(&provider_name)
        .ok_or_else(|| format!("unknown provider: {}", provider_name))?;
    secrets::save(p.env_var, key.trim())
}

#[tauri::command]
fn set_llm_provider(provider: String) -> Result<(), String> {
    if corrector::find_provider(&provider).is_none() {
        return Err(format!("unknown provider: {}", provider));
    }
    let mut config = read_config();
    config["llm_provider"] = serde_json::Value::String(provider.clone());
    save_config(&config)?;
    debug_log::log(&format!("llm_provider set to: {}", provider));
    Ok(())
}

#[tauri::command]
fn list_llm_providers() -> Vec<serde_json::Value> {
    corrector::PROVIDERS
        .iter()
        .map(|p| serde_json::json!({
            "name": p.name,
            "label": p.label,
            "default_model": p.default_model,
        }))
        .collect()
}

#[tauri::command]
fn get_log_history(limit: usize) -> Vec<serde_json::Value> {
    let cap = if limit == 0 { usize::MAX } else { limit };
    logger::read_recent_entries(cap, history_days())
}

#[tauri::command]
fn set_history_days(days: i64) -> Result<(), String> {
    let d = days.clamp(1, 365);
    let mut config = read_config();
    config["history_days"] = serde_json::json!(d);
    save_config(&config)?;
    logger::cleanup_old_logs(d);
    debug_log::log(&format!("history_days set to: {}", d));
    Ok(())
}

#[tauri::command]
fn js_debug_log(msg: String) {
    debug_log::log(&format!("[js] {}", msg));
}

#[tauri::command]
fn get_debug_log() -> String {
    let log_path = match dirs::config_dir() {
        Some(d) => d.join("quill").join("logs").join("debug.log"),
        None => return "Cannot find config directory".to_string(),
    };
    match std::fs::read_to_string(&log_path) {
        Ok(contents) => {
            let lines: Vec<&str> = contents.lines().collect();
            let start = if lines.len() > 200 { lines.len() - 200 } else { 0 };
            lines[start..].join("\n")
        }
        Err(_) => "No debug log found.".to_string(),
    }
}

#[tauri::command]
fn hide_to_tray(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn show_from_tray(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, value: bool) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        w.set_always_on_top(value).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<serde_json::Value, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let body = update.body.clone().unwrap_or_default();
            debug_log::log(&format!("Update available: v{}", version));
            let _ = app.emit("update-available", &version);
            Ok(serde_json::json!({ "available": true, "version": version, "body": body }))
        }
        Ok(None) => {
            debug_log::log("No update available");
            Ok(serde_json::json!({ "available": false }))
        }
        Err(e) => {
            debug_log::log(&format!("Update check failed: {}", e));
            Err(e.to_string())
        }
    }
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            debug_log::log(&format!("Downloading update v{}...", update.version));
            let mut downloaded: u64 = 0;
            let app_for_event = app.clone();
            update
                .download_and_install(
                    move |chunk, total| {
                        downloaded += chunk as u64;
                        let progress = total.map(|t| (downloaded as f64 / t as f64 * 100.0) as u32);
                        let _ = app_for_event.emit("update-progress", progress.unwrap_or(0));
                    },
                    || debug_log::log("Update downloaded, restarting..."),
                )
                .await
                .map_err(|e| {
                    debug_log::log(&format!("Update install failed: {}", e));
                    e.to_string()
                })?;
            app.restart();
        }
        Ok(None) => Err("No update available".into()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn get_current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
fn get_shortcut(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> String {
    state.lock().unwrap().current_shortcut.clone()
}

#[tauri::command]
fn set_shortcut(
    app: AppHandle,
    shortcut: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let new_shortcut: Shortcut = shortcut.parse().map_err(|e| format!("Invalid shortcut: {}", e))?;

    let old_str = state.lock().unwrap().current_shortcut.clone();
    if let Ok(old) = old_str.parse::<Shortcut>() {
        let _ = app.global_shortcut().unregister(old);
    }

    if let Err(e) = register_shortcut(&app, new_shortcut) {
        if let Ok(old) = old_str.parse::<Shortcut>() {
            let _ = register_shortcut(&app, old);
        }
        return Err(e);
    }

    state.lock().unwrap().current_shortcut = shortcut.clone();

    let mut config = read_config();
    config["shortcut"] = serde_json::Value::String(shortcut.clone());
    save_config(&config)?;

    debug_log::log(&format!("Shortcut changed to: {}", shortcut));
    Ok(())
}

/// Hotkey entry point: grab the selection and open the chat window at the
/// cursor. The LLM round-trip is kicked off from the chat (editor_correct), so
/// this only does the fast capture. Runs on its own thread so the hotkey handler
/// never blocks; re-entrancy is guarded by `AppState::busy`.
///
/// The window opens no matter what — a hotkey that does nothing reads as
/// "broken". If Accessibility isn't granted (so the synthetic ⌘C can't read the
/// selection), we pop the real macOS prompt and open the chat with a short note
/// instead of failing silently.
fn launch_editor(state: &Arc<Mutex<AppState>>, app: &AppHandle) {
    {
        let mut s = state.lock().unwrap();
        if s.busy {
            return;
        }
        s.busy = true;
    }

    let app = app.clone();
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        // Position at the cursor and show — both are AppKit work, so marshal
        // them onto the main thread. Positioning before show means the window
        // appears under the mouse on the current Space, not wherever it last sat.
        let show_editor = || {
            let Some(w) = app.get_webview_window("editor") else {
                debug_log::log("editor window missing — cannot open");
                return false;
            };
            let _ = app.run_on_main_thread(move || {
                let _ = mac_window::position_at_cursor(&w);
                let _ = w.show();
                let _ = w.set_focus();
            });
            true
        };

        // No Accessibility → the synthetic ⌘C can't read the selection. Rather
        // than fail silently (the old behaviour that made the hotkey look dead),
        // ask macOS for the grant via its own dialog and open the chat with a
        // one-line note. No half-screen overlay — the system prompt is enough.
        if !accessibility::is_trusted() {
            debug_log::log("hotkey fired → accessibility not granted; prompting");
            accessibility::prompt();
            if show_editor() {
                let _ = app.emit("editor:need-access", ());
            }
            state.lock().unwrap().busy = false;
            return;
        }

        // Let the hotkey's modifier keys fully release before we synthesize ⌘C —
        // otherwise the OS may still see ctrl/alt held and copy a different chord.
        std::thread::sleep(std::time::Duration::from_millis(60));
        debug_log::log("hotkey fired → capturing selection");

        let text = match selection::capture() {
            Ok(t) => t,
            Err(e) => {
                debug_log::log(&format!("capture error: {}", e));
                String::new()
            }
        };
        debug_log::log(&format!("captured {} chars", text.chars().count()));

        // Open even on an empty capture: the user gets a chat to type or paste
        // into instead of a dead key press.
        if show_editor() {
            let _ = app.emit("editor:capture", &text);
        }

        state.lock().unwrap().busy = false;
    });
}

/// Resolve the active provider and its API key from config + env. Shared by the
/// editor's correction commands.
fn resolve_provider_and_key() -> Result<(&'static corrector::ProviderConfig, String), String> {
    let cfg = read_config();
    let provider_name = cfg["llm_provider"]
        .as_str()
        .unwrap_or(corrector::DEFAULT_PROVIDER);
    let provider = corrector::find_provider(provider_name)
        .unwrap_or_else(|| corrector::find_provider(corrector::DEFAULT_PROVIDER).unwrap());
    let key = std::env::var(provider.env_var).unwrap_or_default();
    if key.is_empty() {
        return Err(format!("Нет ключа API для {} — открой настройки Quill.", provider.label));
    }
    Ok((provider, key))
}

/// Correct a chat message and record the pair in history. Async + spawn_blocking
/// so the chat UI keeps animating during the LLM round-trip. Unchanged text
/// ("already clean") isn't logged — there's nothing to keep.
#[tauri::command]
async fn editor_correct(text: String) -> Result<String, String> {
    let (provider, key) = resolve_provider_and_key()?;
    let original = text.clone();
    let corrected =
        tauri::async_runtime::spawn_blocking(move || corrector::correct_text(&text, provider, &key))
            .await
            .map_err(|e| format!("correction task failed: {}", e))??;
    if corrected != original {
        logger::log_correction(&original, &corrected);
    }
    Ok(corrected)
}

/// Put text on the clipboard — the chat's "click a bubble to copy" action, so
/// the user pastes the result wherever they want. Reuses arboard (same crate
/// the capture path borrows the clipboard with).
#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| e.to_string())
}

/// Cancel: hide the chat window.
#[tauri::command]
fn close_editor(app: AppHandle) {
    if let Some(w) = app.get_webview_window("editor") {
        let _ = w.hide();
    }
}

/// Open the settings window (the chat's gear). Lands straight on the settings
/// view rather than the history, via the `show-settings` event.
#[tauri::command]
fn show_main_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = app.emit("show-settings", ());
    }
}

/// Is the app trusted for Accessibility right now? Backs the editor's "I've
/// enabled it" retry button so it can confirm without guessing.
#[tauri::command]
fn accessibility_status() -> bool {
    accessibility::is_trusted()
}

/// Jump straight to the Accessibility pane in System Settings.
#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

/// On a dev machine where `~/membeme/system/secrets/routerai.key` exists, seed
/// the RouterAI key into the keychain on first launch. Quiet no-op otherwise.
fn bootstrap_routerai_key() {
    let Some(home) = dirs::home_dir() else { return };
    let src = home.join("membeme/system/secrets/routerai.key");
    let Ok(key) = std::fs::read_to_string(&src) else { return };
    let key = key.trim();
    if key.is_empty() {
        return;
    }
    if secrets::save("ROUTERAI_API_KEY", key).is_ok() {
        debug_log::log("bootstrapped ROUTERAI_API_KEY from ~/membeme/system/secrets/routerai.key");
    }
}

fn config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("quill").join("config.json"))
}

/// Days of correction history to keep on disk. Default 7.
fn history_days() -> i64 {
    read_config()["history_days"].as_i64().unwrap_or(7).clamp(1, 365)
}

fn read_config() -> serde_json::Value {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::json!({}))
}

fn save_config(config: &serde_json::Value) -> Result<(), String> {
    let path = config_path().ok_or("Cannot find config directory")?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, serde_json::to_string_pretty(config).unwrap()).map_err(|e| e.to_string())
}

/// Show/hide the chat from the tray — the chat *is* the app's face, so the tray
/// toggles it (not the settings window). Mirrors Ribbit: we avoid
/// minimize/unminimize on macOS (it forces a Space switch); hide()/show() lands
/// on the user's current Space. No cursor repositioning here — that's only for
/// the hotkey, which opens the chat where you're working.
fn toggle_chat_window<R: tauri::Runtime>(app: &AppHandle<R>, label: &tauri::menu::MenuItem<R>) {
    let Some(w) = app.get_webview_window("editor") else { return };
    let visible = w.is_visible().unwrap_or(false);
    let focused = w.is_focused().unwrap_or(false);
    if visible && focused {
        let _ = w.hide();
        let _ = label.set_text("Show Quill");
    } else {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = label.set_text("Hide Quill");
    }
}

fn register_shortcut(app: &AppHandle, shortcut: Shortcut) -> Result<(), String> {
    use tauri_plugin_global_shortcut::ShortcutState;
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            // Fire on Release so the chord's modifiers are up before we
            // synthesize ⌘C. Pressed is ignored.
            if event.state() == ShortcutState::Released {
                let state = app.state::<Arc<Mutex<AppState>>>();
                launch_editor(state.inner(), app);
            }
        })
        .map_err(|e| {
            debug_log::log(&format!("shortcut registration failed: {}", e));
            e.to_string()
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    debug_log::log("=== Quill starting ===");
    logger::cleanup_old_logs(history_days());
    tcc_reset::ensure_permissions(BUNDLE_ID);

    // API keys from the OS keychain into the process env (corrector reads env).
    secrets::load_into_env();
    if std::env::var("ROUTERAI_API_KEY").is_err() {
        bootstrap_routerai_key();
    }

    // Warm the TLS handshake so the first correction isn't slow.
    std::thread::spawn(corrector::warm_up_client);

    let config = read_config();
    let saved_shortcut = config["shortcut"]
        .as_str()
        .unwrap_or(DEFAULT_SHORTCUT)
        .to_string();

    let state = Arc::new(Mutex::new(AppState {
        busy: false,
        current_shortcut: saved_shortcut,
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_api_key,
            set_llm_provider,
            list_llm_providers,
            get_log_history,
            set_history_days,
            get_debug_log,
            js_debug_log,
            get_shortcut,
            set_shortcut,
            hide_to_tray,
            show_from_tray,
            set_always_on_top,
            check_for_update,
            install_update,
            get_current_version,
            editor_correct,
            copy_to_clipboard,
            close_editor,
            show_main_window,
            accessibility_status,
            open_accessibility_settings
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // System tray
            let show = MenuItemBuilder::with_id("show", "Show Quill").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit Quill").build(app)?;
            let menu = MenuBuilder::new(app).item(&show).item(&quit).build()?;

            let show_for_menu = show.clone();
            let show_for_tray = show.clone();

            let mut tray_builder = TrayIconBuilder::with_id("tray")
                .tooltip("Quill — polish your writing")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| {
                    if event.id() == "show" {
                        toggle_chat_window(app, &show_for_menu);
                    } else if event.id() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_chat_window(tray.app_handle(), &show_for_tray);
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let _tray = tray_builder.build(app)?;

            // macOS-only window polish: rounded corners + follow active Space.
            // Both windows are borderless+transparent, so both need it.
            for label in ["main", "editor"] {
                if let Some(win) = app.get_webview_window(label) {
                    if let Err(e) = mac_window::apply_rounded_corners(&win, 10.0) {
                        debug_log::log(&format!("rounded corners [{}]: {}", label, e));
                    }
                    if let Err(e) = mac_window::apply_spaces_behavior(&win) {
                        debug_log::log(&format!("spaces behavior [{}]: {}", label, e));
                    }
                }
            }

            app.manage(Arc::clone(&state));

            // Register the saved (or default) hotkey.
            let shortcut_str = state.lock().unwrap().current_shortcut.clone();
            let shortcut: Shortcut = shortcut_str
                .parse()
                .map_err(|e| format!("Failed to parse shortcut: {}", e))?;
            debug_log::log(&format!("registering hotkey: {}", shortcut_str));
            register_shortcut(&handle, shortcut)?;

            // Tray app: launch into the tray, no window in your face — important
            // because every update restarts the app. The one exception is
            // first-run with no API key: pop settings so the hotkey isn't a dead
            // end. Both windows are visible:false in tauri.conf.
            let cfg = read_config();
            let provider_name = cfg["llm_provider"]
                .as_str()
                .unwrap_or(corrector::DEFAULT_PROVIDER);
            let active = corrector::find_provider(provider_name)
                .unwrap_or_else(|| corrector::find_provider(corrector::DEFAULT_PROVIDER).unwrap());
            if !secrets::has_key(active.env_var) {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                    let _ = handle.emit("show-settings", ());
                }
            }

            // Auto-check for updates a few seconds after launch, then every
            // 30 min until one is found — Quill lives in the tray all day.
            let update_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                loop {
                    match update_handle.updater() {
                        Ok(updater) => match updater.check().await {
                            Ok(Some(update)) => {
                                debug_log::log(&format!("update: v{} available", update.version));
                                let _ = update_handle.emit("update-available", &update.version);
                                break;
                            }
                            Ok(None) => debug_log::log("update: up to date"),
                            Err(e) => debug_log::log(&format!("update: auto-check failed: {}", e)),
                        },
                        Err(e) => debug_log::log(&format!("update: auto-check error: {}", e)),
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(30 * 60)).await;
                }
            });

            debug_log::log("setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Quill");
}

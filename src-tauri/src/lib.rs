//! Quill — polish your writing in place.
//!
//! Select text in any app, press the global hotkey, and Quill sends the
//! selection to an LLM that fixes spelling/punctuation/grammar (RU + EN,
//! without changing meaning or tone) and shows the result in a chat at the
//! cursor; you click a bubble to copy it and paste it yourself.
//!
//! Where the pieces live:
//! - selection.rs — grab the current selection (synthetic Copy + clipboard)
//! - corrector.rs — call one LLM endpoint, return corrected text
//! - fallback.rs  — the ordered provider stack + auto-switch on 429/5xx/timeout
//! - logger.rs    — local history of corrections (original → corrected)
//! - secrets.rs   — API keys in a local config file (0600)
//!
//! One window, the chat (src/editor.{html,js}); its settings (model, key,
//! hotkey, updates, debug) live behind the gear as an in-window overlay, not a
//! second window. Copying the result instead of typing it back keeps the
//! Accessibility reach to capture-only (the type-back grant broke every update).
//!
//! Forked from Ribbit (voice-to-text); the tray/updater/window/TCC plumbing is
//! shared, the audio pipeline is replaced by the selection→correct→chat flow.

mod accessibility;
mod corrector;
mod debug_log;
mod fallback;
mod logger;
mod mac_window;
mod secrets;
mod selection;
mod tcc_reset;

use std::sync::{Arc, Mutex};
use tauri::{
    AppHandle, Emitter, Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_updater::UpdaterExt;

/// Menu-bar icon tinted green while an update is waiting — the same signal
/// Ribbit and CopyPaster give, so the three apps behave alike.
const TRAY_UPDATE_ICON: &[u8] = include_bytes!("../icons/tray-update.png");

/// The tray's update item, kept reachable so `announce_update` can rewrite it.
/// A newtype because Tauri keys managed state by type, and the "Show Quill"
/// item already occupies plain `MenuItem<Wry>`.
struct UpdateItem(tauri::menu::MenuItem<tauri::Wry>);

const BUNDLE_ID: &str = "com.quill.app";
const DEFAULT_SHORTCUT: &str = "ctrl+alt+e";

struct AppState {
    /// True while a capture is in flight — guards against the hotkey re-firing
    /// (key repeat, double-tap) before the previous run finishes.
    busy: bool,
    current_shortcut: String,
}

/// Stack entries as JSON for the settings panel, each tagged with whether its
/// key is set — so a card shows a "saved" chip instead of an input, without ever
/// handing the key back to the frontend.
fn stack_json(cfg: &serde_json::Value) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = fallback::read_stack(cfg)
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id, "label": e.label, "url": e.url,
                "model": e.model, "key_env": e.key_env,
                "has_key": secrets::has_key(&e.key_env),
            })
        })
        .collect();
    serde_json::Value::Array(arr)
}

/// Live fallback status for the settings panel: which entry we sit on and how
/// long until the cooldown returns us to the primary. `null` while on primary.
fn stack_state_json(cfg: &serde_json::Value) -> serde_json::Value {
    match fallback::snapshot() {
        Some((active, ago)) => {
            let remaining = fallback::cooldown(cfg).as_secs().saturating_sub(ago.as_secs());
            serde_json::json!({
                "active": active,
                "total": fallback::read_stack(cfg).len(),
                "remaining_secs": remaining,
            })
        }
        None => serde_json::Value::Null,
    }
}

/// Smallest unused `p<N>` id in the stack — stable, no clock needed.
fn next_provider_id(cfg: &serde_json::Value) -> String {
    let max = fallback::read_stack(cfg)
        .iter()
        .filter_map(|e| e.id.strip_prefix('p').and_then(|d| d.parse::<u64>().ok()))
        .max()
        .unwrap_or(0);
    format!("p{}", max + 1)
}

#[tauri::command]
fn get_config() -> Result<serde_json::Value, String> {
    let cfg = read_config();
    let has_api_key = fallback::read_stack(&cfg).iter().any(|e| secrets::has_key(&e.key_env));

    Ok(serde_json::json!({
        "has_api_key": has_api_key,
        "providers": stack_json(&cfg),
        "fallback_threshold": fallback::threshold(&cfg),
        "fallback_cooldown_mins": fallback::cooldown(&cfg).as_secs() / 60,
        "fallback_state": stack_state_json(&cfg),
        "history_days": history_days(),
    }))
}

/// Known providers for the "+ add model" picker. Picking one prefills
/// url/model/key slot; every field stays editable on the card afterwards.
#[tauri::command]
fn list_provider_catalog() -> Vec<serde_json::Value> {
    corrector::PROVIDERS
        .iter()
        .map(|p| serde_json::json!({
            "name": p.name,
            "label": p.label,
            "default_model": p.default_model,
        }))
        .collect()
}

/// Append a provider to the stack. `provider` is a catalog name (prefilled) or
/// "custom" (blank url/model, its own key slot). Returns the updated stack so
/// the UI re-renders from one source of truth.
#[tauri::command]
fn add_provider(provider: String) -> Result<serde_json::Value, String> {
    let mut config = read_config();
    let id = next_provider_id(&config);
    let entry = if provider == "custom" {
        serde_json::json!({
            "id": id, "label": "custom", "url": "", "model": "",
            "key_env": format!("QUILL_KEY_{}", id),
        })
    } else {
        let p = corrector::find_provider(&provider)
            .ok_or_else(|| format!("unknown provider: {}", provider))?;
        serde_json::json!({
            "id": id, "label": p.label, "url": p.base_url,
            "model": p.default_model, "key_env": p.env_var,
        })
    };
    if !config[fallback::CONFIG_KEY].is_array() {
        config[fallback::CONFIG_KEY] = serde_json::json!([]);
    }
    config[fallback::CONFIG_KEY].as_array_mut().unwrap().push(entry);
    save_config(&config)?;
    debug_log::log(&format!("add_provider {} -> {}", provider, id));
    Ok(stack_json(&config))
}

#[tauri::command]
fn remove_provider(id: String) -> Result<serde_json::Value, String> {
    let mut config = read_config();
    if let Some(arr) = config[fallback::CONFIG_KEY].as_array_mut() {
        arr.retain(|e| e.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
    }
    save_config(&config)?;
    debug_log::log(&format!("remove_provider {}", id));
    Ok(stack_json(&config))
}

/// Edit one editable field (url / model / label) of a stack entry.
#[tauri::command]
fn set_provider_field(id: String, field: String, value: String) -> Result<(), String> {
    if !matches!(field.as_str(), "url" | "model" | "label") {
        return Err(format!("field not editable: {}", field));
    }
    let mut config = read_config();
    let arr = config[fallback::CONFIG_KEY].as_array_mut().ok_or("no providers configured")?;
    let entry = arr
        .iter_mut()
        .find(|e| e.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
        .ok_or("unknown provider entry")?;
    entry[field.as_str()] = serde_json::Value::String(value.trim().to_string());
    save_config(&config)?;
    debug_log::log(&format!("set_provider_field {}/{}", id, field));
    Ok(())
}

/// Move an entry up or down — the order IS the fallback priority.
#[tauri::command]
fn move_provider(id: String, up: bool) -> Result<serde_json::Value, String> {
    let mut config = read_config();
    let arr = config[fallback::CONFIG_KEY].as_array_mut().ok_or("no providers configured")?;
    let pos = arr
        .iter()
        .position(|e| e.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
        .ok_or("unknown provider entry")?;
    let target = if up {
        pos.checked_sub(1)
    } else if pos + 1 < arr.len() {
        Some(pos + 1)
    } else {
        None
    };
    if let Some(t) = target {
        arr.swap(pos, t);
        save_config(&config)?;
    }
    Ok(stack_json(&config))
}

/// Store a stack entry's API key in its own slot in the config file.
#[tauri::command]
fn set_provider_key(id: String, key: String) -> Result<(), String> {
    let entry = fallback::read_stack(&read_config())
        .into_iter()
        .find(|e| e.id == id)
        .ok_or("unknown provider entry")?;
    secrets::save(&entry.key_env, key.trim())
}

#[tauri::command]
fn set_fallback_threshold(value: u64) -> Result<(), String> {
    let mut config = read_config();
    config["fallback_threshold"] = serde_json::json!(value.clamp(1, 100));
    save_config(&config)
}

#[tauri::command]
fn set_fallback_cooldown(minutes: u64) -> Result<(), String> {
    let mut config = read_config();
    config["fallback_cooldown_mins"] = serde_json::json!(minutes.clamp(1, 1440));
    save_config(&config)
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

/// Looks for a release and, if one is there, lights the tray. Not a command any
/// more: updating lives in the menu-bar menu, so the window never asks for it.
async fn check_for_update(app: &AppHandle) -> Result<Option<String>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            debug_log::log(&format!("update: v{} available", version));
            announce_update(app, &version);
            Ok(Some(version))
        }
        Ok(None) => {
            debug_log::log("update: up to date");
            Ok(None)
        }
        Err(e) => {
            debug_log::log(&format!("update: check failed: {}", e));
            Err(e.to_string())
        }
    }
}

async fn install_update(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            debug_log::log(&format!("update: downloading v{}", update.version));
            update
                .download_and_install(|_, _| {}, || debug_log::log("update: downloaded, restarting"))
                .await
                .map_err(|e| {
                    debug_log::log(&format!("update: install failed: {}", e));
                    e.to_string()
                })?;
            app.restart();
        }
        Ok(None) => Err("No update available".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Light the menu-bar icon green and turn the menu's update item into the
/// install action. Called from both the manual check and the background poll —
/// one place, so a release found either way gives the user the same signal.
fn announce_update(app: &AppHandle, version: &str) {
    if let Some(item) = app.try_state::<UpdateItem>() {
        let _ = item.0.set_text(format!("Update to v{}", version));
    }
    if let Some(tray) = app.tray_by_id("tray") {
        if let Ok(icon) = tauri::image::Image::from_bytes(TRAY_UPDATE_ICON) {
            let _ = tray.set_icon(Some(icon));
        }
    }
}

/// One menu item, two jobs: check while nothing is pending, install once a
/// version has been found. Two items would leave a dead "Check" sitting next to
/// a live "Update".
async fn on_update_clicked(app: AppHandle) {
    match check_for_update(&app).await {
        Ok(Some(_)) => {
            let _ = install_update(&app).await;
        }
        Ok(None) => debug_log::log("update: nothing to install"),
        Err(e) => debug_log::log(&format!("update: check failed: {}", e)),
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
        debug_log::log(&format!(
            "hotkey fired → capturing selection (frontmost: {})",
            mac_window::frontmost_app()
        ));

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

/// Correct a chat message and record the pair in history. Async + spawn_blocking
/// so the chat UI keeps animating during the LLM round-trip. Unchanged text
/// ("already clean") isn't logged — there's nothing to keep.
///
/// The call walks the provider stack from wherever the sticky fallback state
/// left us: a rate-limited or dead primary hands this correction to the next
/// entry instead of failing it in the user's face.
#[tauri::command]
async fn editor_correct(text: String) -> Result<String, String> {
    let cfg = read_config();
    let entries = fallback::read_stack(&cfg);
    if entries.is_empty() {
        return Err("No model configured — open Quill settings.".into());
    }
    let start = fallback::active_index(fallback::cooldown(&cfg)).min(entries.len() - 1);
    let threshold = fallback::threshold(&cfg);

    let original = text.clone();
    let (corrected, used) = tauri::async_runtime::spawn_blocking(move || {
        fallback::run_with_failover(&entries, start, threshold, |e, key| {
            corrector::correct_text(&text, &e.url, &e.model, key)
        })
    })
    .await
    .map_err(|e| format!("correction task failed: {}", e))??;

    if used != 0 {
        debug_log::log(&format!("corrected via fallback entry #{}", used + 1));
    }
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
/// the RouterAI key into the config file on first launch. Quiet no-op otherwise.
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

/// Build a stack out of catalog names, in order. Unknown names are dropped
/// rather than faked — a typo here must not silently produce a dead entry.
fn seed_stack(names: &[&str]) -> serde_json::Value {
    let entries: Vec<serde_json::Value> = names
        .iter()
        .filter_map(|n| corrector::find_provider(n))
        .enumerate()
        .map(|(i, p)| {
            serde_json::json!({
                "id": format!("p{}", i + 1),
                "label": p.label,
                "url": p.base_url,
                "model": p.default_model,
                "key_env": p.env_var,
            })
        })
        .collect();
    serde_json::Value::Array(entries)
}

/// Bring a config from the single-provider era (a bare `llm_provider` name) up
/// to the provider stack, and seed a fresh install. Runs on every launch and is
/// a no-op once `providers` exists, so it can't churn the file.
///
/// The seed is Groq first (LPU inference — the round-trip stops being felt) with
/// the user's previous provider right behind it as the backup. Groq starts
/// keyless until a key is pasted; a keyless entry is skipped by the stack walk,
/// so the backup keeps working meanwhile and the update can't break a working
/// install.
fn migrate_providers() {
    let mut cfg = read_config();
    if cfg[fallback::CONFIG_KEY].is_array() {
        return;
    }
    let legacy = cfg["llm_provider"].as_str().unwrap_or(corrector::DEFAULT_PROVIDER).to_string();
    let backup = if corrector::find_provider(&legacy).is_some() && legacy != "groq" {
        legacy
    } else {
        corrector::DEFAULT_PROVIDER.to_string()
    };
    cfg[fallback::CONFIG_KEY] = seed_stack(&["groq", &backup]);
    // The old key stays in the same slot in the config file, so the backup entry
    // keeps working without the user re-pasting anything.
    if let Err(e) = save_config(&cfg) {
        debug_log::log(&format!("provider migration failed: {}", e));
        return;
    }
    debug_log::log(&format!("migrated config to provider stack (groq + {})", backup));
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

    // Provider stack first: the key load below needs every slot the stack
    // references, including a custom entry's own slot.
    migrate_providers();
    let key_slots: Vec<String> = fallback::read_stack(&read_config())
        .into_iter()
        .map(|e| e.key_env)
        .collect();
    secrets::load_into_env(&key_slots);
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
            list_provider_catalog,
            add_provider,
            remove_provider,
            set_provider_field,
            move_provider,
            set_provider_key,
            set_fallback_threshold,
            set_fallback_cooldown,
            get_log_history,
            set_history_days,
            get_debug_log,
            js_debug_log,
            get_shortcut,
            set_shortcut,
            get_current_version,
            editor_correct,
            copy_to_clipboard,
            close_editor,
            accessibility_status,
            open_accessibility_settings
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // macOS: menu-bar accessory (no Dock icon, no Cmd-Tab) — same as
            // Ribbit. Quill is summoned by a hotkey and gets out of the way; a
            // regular app also activates itself when its window shows, and that
            // pulls focus off the text being corrected.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // System tray. A left click opens the menu — same as Ribbit and
            // CopyPaster, so "click the animal, get a menu" holds everywhere.
            // The update line is the only place updating lives; the window has
            // no button for it.
            let update = MenuItemBuilder::with_id("update", "Check for updates").build(app)?;
            let show = MenuItemBuilder::with_id("show", "Show Quill").build(app)?;
            let version = MenuItemBuilder::with_id("version", format!("Quill v{}", env!("CARGO_PKG_VERSION")))
                .enabled(false)
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit Quill").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&update)
                .separator()
                .item(&show)
                .separator()
                .item(&version)
                .item(&quit)
                .build()?;

            let show_for_menu = show.clone();
            // announce_update() rewrites this item's text when a release lands.
            app.manage(UpdateItem(update.clone()));

            let mut tray_builder = TrayIconBuilder::with_id("tray")
                .tooltip("Quill — polish your writing")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "update" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            on_update_clicked(app).await;
                        });
                    }
                    "show" => toggle_chat_window(app, &show_for_menu),
                    "quit" => app.exit(0),
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let _tray = tray_builder.build(app)?;

            // macOS-only window polish: rounded corners + follow active Space.
            // One window now — the chat (borderless + transparent).
            if let Some(win) = app.get_webview_window("editor") {
                if let Err(e) = mac_window::apply_rounded_corners(&win, 10.0) {
                    debug_log::log(&format!("rounded corners: {}", e));
                }
                if let Err(e) = mac_window::apply_spaces_behavior(&win) {
                    debug_log::log(&format!("spaces behavior: {}", e));
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
            // first-run with no key anywhere in the stack: reveal the chat
            // (visible:false in tauri.conf) so the hotkey isn't a dead end;
            // editor.js sees the missing key and opens the settings overlay.
            let no_key = !fallback::read_stack(&read_config())
                .iter()
                .any(|e| secrets::has_key(&e.key_env));
            if no_key {
                if let Some(w) = app.get_webview_window("editor") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            // Auto-check for updates a few seconds after launch, then every
            // 30 min until one is found — Quill lives in the tray all day.
            let update_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                loop {
                    // Found one → the tray is already lit and the menu item says
                    // "Update to vX"; nothing left to poll for.
                    if let Ok(Some(_)) = check_for_update(&update_handle).await {
                        break;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The update signal is the icon itself — ship the plain pen by mistake and
    /// the user never learns an update is waiting, silently and forever.
    #[test]
    fn the_update_icon_carries_the_green_badge() {
        let icon = tauri::image::Image::from_bytes(TRAY_UPDATE_ICON).expect("tray-update.png decodes");
        let badge = icon.rgba().chunks(4).any(|px| (px[0], px[1], px[2], px[3]) == (46, 204, 113, 255));
        assert!(badge, "no #2ecc71 pixels — is this the plain icon?");
    }

    #[test]
    fn seed_stack_is_groq_first_backup_second() {
        let s = seed_stack(&["groq", "routerai"]);
        let arr = s.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["label"], "Groq");
        assert_eq!(arr[0]["model"], "llama-3.3-70b-versatile");
        assert_eq!(arr[0]["key_env"], "GROQ_API_KEY");
        assert_eq!(arr[0]["id"], "p1");
        assert_eq!(arr[1]["label"], "RouterAI");
        assert_eq!(arr[1]["id"], "p2");
    }

    #[test]
    fn seed_stack_drops_unknown_names() {
        assert_eq!(seed_stack(&["groq", "nonesuch"]).as_array().unwrap().len(), 1);
    }

    #[test]
    fn next_provider_id_skips_used_ids() {
        let cfg = serde_json::json!({
            "providers": [
                {"id": "p1", "url": "u", "key_env": "K"},
                {"id": "p4", "url": "u", "key_env": "K"}
            ]
        });
        assert_eq!(next_provider_id(&cfg), "p5");
        assert_eq!(next_provider_id(&serde_json::json!({})), "p1");
    }
}

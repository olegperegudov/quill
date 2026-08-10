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
mod private;
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

/// Menu-bar icon tinted green while an update is waiting — the same signal
/// Ribbit and CopyPaster give, so the three apps behave alike.
const TRAY_UPDATE_ICON: &[u8] = include_bytes!("../icons/tray-update.png");

/// The tray's update item, kept reachable so `announce_update` can rewrite it.
/// A newtype because Tauri keys managed state by type: a bare `MenuItem<Wry>`
/// would be ambiguous the moment a second item wants to be managed too.
struct UpdateItem(tauri::menu::MenuItem<tauri::Wry>);

const BUNDLE_ID: &str = "com.quill.app";
const DEFAULT_SHORTCUT: &str = "ctrl+alt+e";

/// Every release, newest first, each with what changed in it and its installers
/// — where the menu's version item goes. The list, not a single tag: someone who
/// has just been offered an update wants to read the version above theirs.
const RELEASES_URL: &str = "https://github.com/olegperegudov/quill/releases";

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

/// An endpoint is where the API key goes on every request. Over plain http the
/// key crosses the network in the clear, so a non-https endpoint is refused when
/// it is typed — refusing it at request time is too late, it has already been sent.
fn require_https(url: &str) -> Result<(), String> {
    let u = url.trim();
    if u.is_empty() || u.starts_with("https://") {
        return Ok(());
    }
    Err("endpoint must start with https:// — your key travels with every request".into())
}

/// Edit one editable field (url / model / label) of a stack entry.
#[tauri::command]
fn set_provider_field(id: String, field: String, value: String) -> Result<(), String> {
    if !matches!(field.as_str(), "url" | "model" | "label") {
        return Err(format!("field not editable: {}", field));
    }
    if field == "url" {
        require_https(&value)?;
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

/// The instruction in force, plus what settings needs to show around it: the
/// one Quill ships with (so "reset" has something to restore) and the guard it
/// always appends (so the panel can say what the user cannot switch off).
#[tauri::command]
fn get_prompt() -> serde_json::Value {
    serde_json::json!({
        "instruction": prompt_instruction(&read_config()),
        "default": corrector::DEFAULT_INSTRUCTION,
        "guard": corrector::PROMPT_GUARD,
    })
}

/// Store the user's instruction. Empty (or whitespace) puts Quill's own back —
/// that is what the reset control sends.
#[tauri::command]
fn set_prompt(instruction: String) -> Result<(), String> {
    let trimmed = instruction.trim();
    let mut config = read_config();
    if trimmed.is_empty() {
        config["prompt"] = serde_json::Value::Null;
    } else {
        config["prompt"] = serde_json::json!(trimmed);
    }
    save_config(&config)?;
    debug_log::log(&format!(
        "prompt set: {}",
        if trimmed.is_empty() { "back to Quill's own".into() } else { format!("{} chars", trimmed.chars().count()) }
    ));
    Ok(())
}

/// The instruction to send with a correction: the user's, or Quill's when they
/// have not written one.
fn prompt_instruction(config: &serde_json::Value) -> String {
    config["prompt"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(corrector::DEFAULT_INSTRUCTION)
        .to_string()
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
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = mac_window::position_at_cursor(&w);
                show_chat(&handle);
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
    let instruction = prompt_instruction(&cfg);

    let original = text.clone();
    let (corrected, used) = tauri::async_runtime::spawn_blocking(move || {
        fallback::run_with_failover(&entries, start, threshold, |e, key| {
            corrector::correct_text(&text, &e.url, &e.model, key, &instruction)
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

/// Cancel: hide the chat window (Esc, the cross in the titlebar).
#[tauri::command]
fn close_editor(app: AppHandle) {
    hide_chat(&app);
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
    let cfg = read_config();
    let Some(migrated) = migrated_config(&cfg) else { return };
    // The old key stays in the same slot in the config file, so the backup entry
    // keeps working without the user re-pasting anything.
    match save_config(&migrated) {
        Ok(()) => debug_log::log("migrated config to the provider stack"),
        Err(e) => debug_log::log(&format!("provider migration failed: {}", e)),
    }
}

/// The migration itself, kept free of IO so it can be pinned by tests — this
/// function rewrites the user's config on every launch, and "does it clobber a
/// working install" is not something to find out in production.
///
/// `None` = the config already has a stack, leave it alone.
fn migrated_config(cfg: &serde_json::Value) -> Option<serde_json::Value> {
    if cfg[fallback::CONFIG_KEY].is_array() {
        return None;
    }
    let legacy = cfg["llm_provider"].as_str().unwrap_or(corrector::DEFAULT_PROVIDER);
    let backup = if corrector::find_provider(legacy).is_some() && legacy != "groq" {
        legacy
    } else {
        corrector::DEFAULT_PROVIDER
    };
    let mut out = cfg.clone();
    out[fallback::CONFIG_KEY] = seed_stack(&["groq", backup]);
    Some(out)
}

/// Windows that hide on close instead of being destroyed. Every window the tray
/// or the hotkey can raise belongs here — a destroyed one cannot be raised again.
/// The test at the bottom of this file reads `tauri.conf.json` and fails if a
/// window is declared there and missing here.
const HIDE_ON_CLOSE: [&str; 1] = ["editor"];

fn config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("quill").join("config.json"))
}

/// Days of correction history to keep on disk. Default 7.
fn history_days() -> i64 {
    read_config()["history_days"].as_i64().unwrap_or(7).clamp(1, 365)
}

/// A config we could not parse is set aside, not overwritten. Empty defaults look
/// exactly like a fresh install to `migrate_providers`, which then re-seeds and
/// saves — so a half-written file (a crash mid-save, a full disk) would silently
/// take the provider stack, the endpoints and the keys with it. Keep the corpse:
/// the user can open it and copy the endpoint back.
fn read_config() -> serde_json::Value {
    let Some(path) = config_path() else { return serde_json::json!({}) };
    let Ok(raw) = std::fs::read_to_string(&path) else { return serde_json::json!({}) };
    match serde_json::from_str(&raw) {
        Ok(cfg) => cfg,
        Err(e) => {
            let kept = path.with_extension("broken.json");
            let _ = std::fs::rename(&path, &kept);
            debug_log::log(&format!("config unreadable ({}), kept as {:?}", e, kept.file_name()));
            serde_json::json!({})
        }
    }
}

fn save_config(config: &serde_json::Value) -> Result<(), String> {
    let path = config_path().ok_or("Cannot find config directory")?;
    private::create_dir(path.parent().unwrap()).map_err(|e| e.to_string())?;
    private::write(&path, serde_json::to_string_pretty(config).unwrap().as_bytes()).map_err(|e| e.to_string())
}

/// A rectangle in physical pixels — a tray icon's, or a screen's.
#[derive(Clone, Copy, Debug)]
struct PixelRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Breathing room between the tray icon and the window, in logical pixels.
const TRAY_GAP: f64 = 6.0;

/// Where a `win_w × win_h` window goes so it hangs off the tray `icon` like that
/// icon's own menu: centred under it, or above it when the icon sits at the
/// bottom of the screen (a Windows taskbar), and never past the screen edge —
/// an icon in the corner would otherwise push half the window off it.
///
/// `screen` is the display the icon is on; None (no monitor reported) simply
/// skips the fitting. Pure geometry, so the placement is tested without a screen.
fn popover_position(icon: PixelRect, win_w: f64, win_h: f64, screen: Option<PixelRect>, gap: f64) -> (f64, f64) {
    let mut x = icon.x + icon.w / 2.0 - win_w / 2.0;
    let mut y = icon.y + icon.h + gap;
    if let Some(s) = screen {
        if y + win_h > s.y + s.h {
            y = (icon.y - gap - win_h).max(s.y + gap);
        }
        let leftmost = s.x + gap;
        let rightmost = (s.x + s.w - win_w - gap).max(leftmost);
        x = x.clamp(leftmost, rightmost);
    }
    (x, y)
}

/// Park the window under the tray icon that was just clicked.
fn anchor_to_tray<R: tauri::Runtime>(w: &tauri::WebviewWindow<R>, rect: tauri::Rect) {
    let scale = w.scale_factor().unwrap_or(1.0);
    let pos = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    let icon = PixelRect { x: pos.x, y: pos.y, w: size.width, h: size.height };
    let Ok(win) = w.outer_size() else { return };
    let screen = w
        .monitor_from_point(icon.x, icon.y)
        .ok()
        .flatten()
        .or_else(|| w.current_monitor().ok().flatten())
        .map(|m| PixelRect {
            x: m.position().x as f64,
            y: m.position().y as f64,
            w: m.size().width as f64,
            h: m.size().height as f64,
        });
    let (x, y) = popover_position(icon, win.width as f64, win.height as f64, screen, TRAY_GAP * scale);
    let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
}

/// When the window last hid itself because focus went elsewhere.
///
/// Clicking the tray icon is one such "elsewhere": the window loses focus and
/// auto-hides *before* the click handler runs, so the handler would see a hidden
/// window and show it right back — the icon could never close it.
static LAST_AUTO_HIDE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Called from the focus-lost handler right before it hides the window.
fn note_auto_hide() {
    if let Ok(mut t) = LAST_AUTO_HIDE.lock() {
        *t = Some(std::time::Instant::now());
    }
}

/// True when the window auto-hid a moment ago — i.e. the click being handled is
/// what dismissed it.
fn just_auto_hid() -> bool {
    LAST_AUTO_HIDE
        .lock()
        .ok()
        .and_then(|t| *t)
        .map(|t| t.elapsed() < std::time::Duration::from_millis(400))
        .unwrap_or(false)
}

/// Bring the chat forward. On macOS the window is a non-activating NSPanel
/// (mac_window::setup_panel): it surfaces on the Space the user is on right now,
/// over a full-screen app included, and takes the keyboard without activating
/// Quill — activating is what used to teleport the user to another desktop.
/// Everywhere else an ordinary show/focus is the whole story.
fn show_chat(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    mac_window::show_panel(app);
    #[cfg(not(target_os = "macos"))]
    if let Some(w) = app.get_webview_window("editor") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Put the chat back in the tray.
fn hide_chat(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    mac_window::hide_panel(app);
    #[cfg(not(target_os = "macos"))]
    if let Some(w) = app.get_webview_window("editor") {
        let _ = w.hide();
    }
}

/// Whether the chat is on screen.
fn chat_visible(app: &AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    {
        mac_window::panel_visible(app)
    }
    #[cfg(not(target_os = "macos"))]
    {
        app.get_webview_window("editor").and_then(|w| w.is_visible().ok()).unwrap_or(false)
    }
}

/// Left click on the tray icon: the chat drops out from under the icon, or goes
/// away again if it was up — the frog's behaviour, and the reason the tray click
/// carries the icon's own rectangle. The hotkey path does not come through here
/// — it opens the chat at the cursor, where the text being corrected is.
fn toggle_chat_window(app: &AppHandle, rect: Option<tauri::Rect>) {
    let Some(w) = app.get_webview_window("editor") else {
        // A window that isn't there reads as a dead icon. Say so in the log.
        debug_log::log("tray: the chat window is gone, cannot show it");
        return;
    };
    // The panel hides itself the moment focus leaves it, and this very click is
    // what took the focus away — so an open chat already reads as hidden by the
    // time we run. `just_auto_hid` is the "the click you are handling is the one
    // that closed it" signal; without it the icon could never close the window.
    if chat_visible(app) || just_auto_hid() {
        hide_chat(app);
        return;
    }
    if let Some(rect) = rect {
        anchor_to_tray(&w, rect);
    }
    show_chat(app);
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
    debug_log::init();
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

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // The window is a desk you size once. Position is saved too, but only
        // decides where it opens on launch — the tray click parks it under the
        // icon and the hotkey puts it at the cursor, where the text is.
        .plugin(tauri_plugin_window_state::Builder::default().build());
    // macOS-only: turns the chat into the non-activating NSPanel (see mac_window).
    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }
    builder
        .on_window_event(|window, event| {
            // Closing hides. Quill has one window and it *is* the app: destroy it
            // (the cross, ⌘W — macOS installs its own Close item when the app sets
            // no menu) and the hotkey opens nothing, the tray item opens nothing,
            // and with no windows left Tauri exits the process — tray and all.
            // The window the tray can raise must always be there to raise.
            if HIDE_ON_CLOSE.contains(&window.label()) {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    hide_chat(window.app_handle());
                }
            }
            // The chat hangs off the tray icon like that icon's own menu, and a
            // menu closes when you look away: a click anywhere else takes focus
            // and the window goes back to the tray, one click from returning.
            // (Esc does the same from the keyboard — editor.js calls
            // close_editor.) `note_auto_hide` marks the moment so the tray click
            // that caused it isn't read as "show me the window".
            //
            // macOS has its own path: the panel's `window_did_resign_key`
            // delegate (mac_window::setup_panel). A non-activating panel does not
            // report focus through Tauri's window events, so this handler is the
            // Windows half of the same behaviour.
            #[cfg(not(target_os = "macos"))]
            if window.label() == "editor" {
                if let tauri::WindowEvent::Focused(false) = event {
                    note_auto_hide();
                    let _ = window.hide();
                }
            }
        })
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
            get_prompt,
            set_prompt,
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

            // System tray, split the way the frog and the parrot split it: the
            // left button is the way into the app (it toggles the chat), the
            // right button is the way to the housekeeping — update, version,
            // quit. Nothing in the window asks for an update; this menu is the
            // only place it lives.
            let update = MenuItemBuilder::with_id("update", "Check for updates").build(app)?;
            // The version is a way in, not a label: it opens the release list,
            // where every build says what changed in it. Deciding whether to
            // install an update used to mean going and finding that out.
            let version = MenuItemBuilder::with_id("version", format!("Quill v{}", env!("CARGO_PKG_VERSION")))
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit Quill").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&update)
                .separator()
                .item(&version)
                .item(&quit)
                .build()?;

            // announce_update() rewrites this item's text when a release lands.
            app.manage(UpdateItem(update.clone()));

            let mut tray_builder = TrayIconBuilder::with_id("tray")
                .tooltip("Quill — polish your writing")
                .menu(&menu)
                // The menu belongs to the right button alone, or it and the
                // window would fight over the same click.
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        // On the release, not the press: the press is also what
                        // takes focus off an open window, and acting on both
                        // toggles twice.
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        toggle_chat_window(tray.app_handle(), Some(rect));
                    }
                })
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "update" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            on_update_clicked(app).await;
                        });
                    }
                    "version" => {
                        use tauri_plugin_opener::OpenerExt;
                        if let Err(e) = app.opener().open_url(RELEASES_URL, None::<&str>) {
                            debug_log::log(&format!("opening the release list failed: {}", e));
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let _tray = tray_builder.build(app)?;

            // macOS-only window work: become the panel first (it replaces the
            // window's whole Space behaviour), then round its corners.
            if let Some(win) = app.get_webview_window("editor") {
                if let Err(e) = mac_window::setup_panel(&win) {
                    debug_log::log(&format!("panel setup: {}", e));
                }
                if let Err(e) = mac_window::apply_rounded_corners(&win, 10.0) {
                    debug_log::log(&format!("rounded corners: {}", e));
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
                show_chat(app.handle());
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

#[cfg(test)]
mod endpoint_tests {
    use super::require_https;

    #[test]
    fn a_plain_http_endpoint_is_refused() {
        assert!(require_https("http://api.example.com/v1").is_err(), "http would send the key in the clear");
        assert!(require_https("ftp://api.example.com").is_err());
        assert!(require_https("api.example.com").is_err(), "no scheme is not a scheme we trust");
    }

    #[test]
    fn https_and_an_empty_field_are_fine() {
        assert!(require_https("https://api.groq.com/openai/v1").is_ok());
        assert!(require_https("  https://api.openai.com/v1 ").is_ok());
        assert!(require_https("").is_ok(), "clearing the field is not an attack");
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    fn stack_labels(cfg: &serde_json::Value) -> Vec<String> {
        cfg[fallback::CONFIG_KEY]
            .as_array()
            .expect("a stack")
            .iter()
            .map(|e| e["label"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn a_fresh_install_gets_groq_in_front() {
        let seeded = migrated_config(&serde_json::json!({})).expect("a fresh config is migrated");
        let labels = stack_labels(&seeded);
        assert_eq!(labels.len(), 2, "front runner plus a backup");
        assert!(labels[0].to_lowercase().contains("groq"), "groq runs first, got {:?}", labels);
    }

    #[test]
    fn the_old_single_provider_stays_as_the_backup() {
        let old = serde_json::json!({ "llm_provider": "openai", "shortcut": "ctrl+alt+e" });
        let seeded = migrated_config(&old).expect("a pre-stack config is migrated");
        let labels = stack_labels(&seeded);
        assert!(labels[1].to_lowercase().contains("openai"), "backup should be openai, got {:?}", labels);
        assert_eq!(seeded["shortcut"], "ctrl+alt+e", "migration must not drop the rest of the config");
    }

    #[test]
    fn a_config_that_already_has_a_stack_is_left_alone() {
        // Runs on every launch: the day it stops being a no-op it starts eating
        // whatever the user arranged.
        let mine = serde_json::json!({ fallback::CONFIG_KEY: [{ "id": "p1", "label": "Mine" }] });
        assert!(migrated_config(&mine).is_none());
    }

    #[test]
    fn migrating_twice_changes_nothing_the_second_time() {
        let once = migrated_config(&serde_json::json!({})).unwrap();
        assert!(migrated_config(&once).is_none(), "the migration is not idempotent");
    }
}

#[cfg(test)]
mod tray_tests {
    /// This file up to its first test module. Both tests below read the source
    /// for lines they also quote in their own assertions — searching the whole
    /// file would find the quote and pass over a deleted builder line.
    fn app_code() -> &'static str {
        include_str!("lib.rs").split("#[cfg(test)]").next().expect("lib.rs has a body")
    }

    /// The three apps share one reflex: left click on the animal opens its
    /// window, right click opens the housekeeping menu. Letting Tauri put the
    /// menu back on the left button is a one-word regression that takes the
    /// window away entirely — the left click would open a menu and the
    /// `TrayIconEvent::Click` handler would never fire.
    #[test]
    fn the_left_click_belongs_to_the_window_not_the_menu() {
        let code = app_code();
        assert!(
            code.contains("show_menu_on_left_click(false)"),
            "the menu is back on the left button — the window has no way in from the tray"
        );
        assert!(
            code.contains("button: MouseButton::Left"),
            "nothing handles the left click; the tray icon does nothing at all"
        );
    }

    /// The chat is a popover: it drops out from under the icon and goes away
    /// when you look elsewhere. Lose the anchor and it opens in the middle of
    /// the screen; lose the focus-lost hide and it stays over everything; lose
    /// the guard and the icon can never close it (the click that dismissed the
    /// window would be read as a request to show it).
    #[test]
    fn the_chat_hangs_off_the_icon_and_closes_when_focus_leaves() {
        let code = app_code();
        assert!(
            code.contains("anchor_to_tray(&w, rect)"),
            "the tray click no longer parks the window under the icon"
        );
        assert!(
            code.contains("tauri::WindowEvent::Focused(false)"),
            "nothing hides the window when focus leaves it"
        );
        assert!(
            code.contains("|| just_auto_hid()"),
            "the tray click ignores the auto-hide it just caused — the icon cannot close the window"
        );
    }

    /// Update, version, quit — the same three the frog and the parrot carry, and
    /// the only place updating lives (the window has no button for it).
    #[test]
    fn the_menu_carries_update_version_and_quit() {
        let code = app_code();
        for id in ["\"update\"", "\"version\"", "\"quit\""] {
            assert!(code.contains(&format!("with_id({}", id)), "tray menu lost {}", id);
        }
    }

    /// The version item is the way to the release list — the only place that
    /// says what an update contains. It was a greyed-out label for a while, and
    /// installing an update meant taking it on trust.
    #[test]
    fn the_version_opens_the_release_list() {
        let code = app_code();
        assert!(
            code.contains("open_url(RELEASES_URL"),
            "the version item no longer opens the release list"
        );
        assert!(
            !code.contains(".enabled(false)"),
            "a menu item is greyed out again — the version item is a link, not a label"
        );
    }
}

/// The window hangs off the tray icon, so where it lands is geometry worth
/// pinning: an icon near a screen edge, on a second monitor, or on a taskbar at
/// the bottom each used to push the window half off the screen.
#[cfg(test)]
mod popover_tests {
    use super::{popover_position, PixelRect};

    /// A 1440p screen with a 24px-tall menu bar icon at x=1200, the ordinary case.
    const SCREEN: PixelRect = PixelRect { x: 0.0, y: 0.0, w: 2560.0, h: 1440.0 };
    const WIN: (f64, f64) = (520.0, 700.0);
    const GAP: f64 = 6.0;

    #[test]
    fn the_window_hangs_centred_under_the_icon() {
        let icon = PixelRect { x: 1200.0, y: 0.0, w: 24.0, h: 24.0 };
        let (x, y) = popover_position(icon, WIN.0, WIN.1, Some(SCREEN), GAP);
        assert_eq!(x, 1212.0 - WIN.0 / 2.0, "icon centre, minus half the window");
        assert_eq!(y, 30.0, "just below the icon");
    }

    #[test]
    fn an_icon_at_the_bottom_of_the_screen_gets_the_window_above_it() {
        // A Windows taskbar: hanging "below" would put the window off-screen.
        let icon = PixelRect { x: 1200.0, y: 1400.0, w: 24.0, h: 24.0 };
        let (_, y) = popover_position(icon, WIN.0, WIN.1, Some(SCREEN), GAP);
        assert_eq!(y, 1400.0 - GAP - WIN.1);
    }

    #[test]
    fn a_corner_icon_does_not_push_the_window_off_the_screen() {
        let right = PixelRect { x: 2548.0, y: 0.0, w: 12.0, h: 24.0 };
        let (x, _) = popover_position(right, WIN.0, WIN.1, Some(SCREEN), GAP);
        assert_eq!(x, SCREEN.w - WIN.0 - GAP);

        let left = PixelRect { x: 0.0, y: 0.0, w: 12.0, h: 24.0 };
        let (x, _) = popover_position(left, WIN.0, WIN.1, Some(SCREEN), GAP);
        assert_eq!(x, GAP);
    }

    #[test]
    fn a_second_monitor_is_measured_from_its_own_origin() {
        // Monitors to the right of the primary start at a non-zero x, and one
        // above it at a negative y — placement must not assume a 0,0 origin.
        let screen = PixelRect { x: 2560.0, y: -1440.0, w: 1920.0, h: 1080.0 };
        let icon = PixelRect { x: 4470.0, y: -1440.0, w: 12.0, h: 24.0 };
        let (x, y) = popover_position(icon, WIN.0, WIN.1, Some(screen), GAP);
        assert_eq!(x, screen.x + screen.w - WIN.0 - GAP);
        assert_eq!(y, -1440.0 + 24.0 + GAP);
    }

    #[test]
    fn without_a_monitor_the_window_still_lands_under_the_icon() {
        let icon = PixelRect { x: 1200.0, y: 0.0, w: 24.0, h: 24.0 };
        let (x, y) = popover_position(icon, WIN.0, WIN.1, None, GAP);
        assert_eq!((x, y), (1212.0 - WIN.0 / 2.0, 30.0));
    }
}

/// How the chat behaves across desktops. Read from `mac_window.rs` because the
/// behaviour is three AppKit flags and a delegate, none of which can be observed
/// without a screen — but all of which can be lost in one edit.
#[cfg(test)]
mod spaces_tests {
    fn panel_code() -> &'static str {
        include_str!("mac_window.rs")
    }

    /// The chat has to arrive on the desktop the user is on and stay off the
    /// others. CanJoinAllSpaces bought the first at the cost of the second: the
    /// window was resident on *every* Space, so swiping to the next desktop
    /// showed it there mid-swipe and then took it away — reported as "Quill
    /// flashes in the middle of the screen when I change desktops". The panel's
    /// MoveToActiveSpace is the mechanism that gets both.
    #[test]
    fn the_chat_follows_the_user_instead_of_living_on_every_desktop() {
        let code = panel_code();
        assert!(
            code.contains("move_to_active_space()"),
            "the panel no longer follows the user to the active Space"
        );
        assert!(
            !code.contains("setCollectionBehavior"),
            "the hand-rolled CanJoinAllSpaces is back — the chat will sit on every desktop again"
        );
    }

    /// Focus leaves → back to the tray. On macOS this is the panel's own
    /// delegate: a non-activating panel never activates the app, so Tauri's
    /// window-focus event does not fire and this is the only place it can hang.
    #[test]
    fn the_panel_hides_itself_when_focus_leaves() {
        let code = panel_code();
        assert!(
            code.contains("window_did_resign_key(move |"),
            "nothing hides the panel when focus leaves it"
        );
        assert!(
            code.contains("nonactivating_panel()"),
            "the chat activates Quill when it opens — that teleports the user to another desktop"
        );
    }
}

#[cfg(test)]
mod window_tests {
    use super::HIDE_ON_CLOSE;

    #[test]
    fn every_window_hides_on_close_instead_of_being_destroyed() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        for w in conf["app"]["windows"].as_array().expect("windows in the config") {
            let label = w["label"].as_str().expect("a window without a label");
            assert!(
                HIDE_ON_CLOSE.contains(&label),
                "window '{}' is not in HIDE_ON_CLOSE: closing it destroys it, and Quill \
                 with no windows left is a dead tray icon",
                label
            );
        }
    }
}

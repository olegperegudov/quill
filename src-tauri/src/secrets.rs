//! API-key storage in a local config file (config_dir/quill/.env), owner-only.
//!
//! Why a file and not the OS keychain: an ad-hoc-signed Tauri app gets a fresh
//! code signature every release, and a macOS Keychain ACL is anchored to that
//! signature. So after each update macOS re-prompts for the login password to
//! re-authorize keychain access — and that post-update keychain re-authorization
//! was disturbing other keychain-backed sessions on the machine (it lined up
//! exactly with a corporate VPN dropping on every Quill update). A plain file
//! never touches the keychain, which is how Ribbit (the upstream app) has always
//! stored its key. An API key can't be hashed (it's sent to the provider as-is),
//! so the realistic choice is keychain vs file; the file is the user's own
//! credential on their own machine, written 0600 (owner read/write only).
//!
//! Public API is unchanged: at startup we load any stored keys into the process
//! env, and corrector.rs keeps reading them via std::env::var(provider.env_var).

use crate::debug_log;
use std::path::{Path, PathBuf};

fn env_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("quill").join(".env"))
}

fn read_file() -> String {
    env_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

/// Value stored for `env_var` in the .env file, if any.
fn from_file(env_var: &str) -> Option<String> {
    value_in(&read_file(), env_var)
}

/// The file half of the lookup, split out so it can be tested without touching
/// the real config directory.
fn value_in(body: &str, env_var: &str) -> Option<String> {
    let prefix = format!("{}=", env_var);
    body.lines().find_map(|l| l.strip_prefix(&prefix).map(str::to_string))
}

/// Pull the stored keys for the given slots into the process environment, so the
/// corrector can read them the usual way. The slots come from the configured
/// provider stack — a custom endpoint owns its own slot, not a catalog one.
pub fn load_into_env(env_vars: &[String]) {
    for env_var in env_vars {
        // Respect an already-exported env var (dev override) over the file.
        if std::env::var(env_var).map(|k| !k.is_empty()).unwrap_or(false) {
            continue;
        }
        if let Some(key) = from_file(env_var) {
            if !key.is_empty() {
                unsafe { std::env::set_var(env_var, &key) };
                debug_log::log(&format!("loaded {} from config", env_var));
            }
        }
    }
}

/// Store a key in the config file and make it live in the current process.
pub fn save(env_var: &str, key: &str) -> Result<(), String> {
    let path = env_path().ok_or("Cannot find config directory")?;
    crate::private::create_dir(path.parent().unwrap()).map_err(|e| e.to_string())?;

    // Rewrite, replacing only this var's line; keep the rest untouched.
    let prefix = format!("{}=", env_var);
    let mut lines: Vec<String> = read_file()
        .lines()
        .filter(|l| !l.starts_with(&prefix) && !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    lines.push(format!("{}={}", env_var, key));
    write_private(&path, &(lines.join("\n") + "\n"))?;

    unsafe { std::env::set_var(env_var, key) };
    debug_log::log(&format!("saved {} to config", env_var));
    Ok(())
}

/// The key to use for this slot: the process env first (startup load and dev
/// overrides land there), the config file behind it.
///
/// Every reader goes through here, including the provider stack at request
/// time. Reading the file only at startup used to mean a key added afterwards —
/// by hand, or by another window — was reported as present by the settings
/// screen while the stack still skipped that provider as keyless until the app
/// was restarted.
pub fn key_for(env_var: &str) -> String {
    match std::env::var(env_var) {
        Ok(k) if !k.is_empty() => k,
        _ => from_file(env_var).unwrap_or_default(),
    }
}

/// Whether a usable key for this env var is present (process env or file).
pub fn has_key(env_var: &str) -> bool {
    !key_for(env_var).is_empty()
}

/// Write the file owner-only (0600) so the token isn't readable by other users.
/// The mode is set on the open handle, before the key goes in — writing first
/// and chmod'ing after leaves a moment where the token is already on disk and
/// still world-readable.
fn write_private(path: &Path, body: &str) -> Result<(), String> {
    crate::private::write(path, body.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_in_reads_only_its_own_slot() {
        let body = "ROUTERAI_API_KEY=rk-1\nGROQ_API_KEY=gk-2\n";
        assert_eq!(value_in(body, "GROQ_API_KEY").unwrap(), "gk-2");
        assert_eq!(value_in(body, "ROUTERAI_API_KEY").unwrap(), "rk-1");
        assert!(value_in(body, "OPENAI_API_KEY").is_none());
        // A slot whose name merely ends the same must not match.
        assert!(value_in(body, "API_KEY").is_none());
    }

    #[test]
    fn env_wins_over_the_file() {
        unsafe { std::env::set_var("QS_T1_KEY", "from-env") };
        assert_eq!(key_for("QS_T1_KEY"), "from-env");
    }

    #[test]
    fn an_empty_env_slot_is_no_key() {
        unsafe { std::env::set_var("QS_T2_KEY", "") };
        assert!(!has_key("QS_T2_KEY"));
    }
}

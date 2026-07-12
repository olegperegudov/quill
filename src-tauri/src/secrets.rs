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
    let prefix = format!("{}=", env_var);
    read_file()
        .lines()
        .find_map(|l| l.strip_prefix(&prefix).map(str::to_string))
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
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;

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

/// Whether a usable key for this env var is present (process env or file).
pub fn has_key(env_var: &str) -> bool {
    if std::env::var(env_var).map(|k| !k.is_empty()).unwrap_or(false) {
        return true;
    }
    from_file(env_var).map(|k| !k.is_empty()).unwrap_or(false)
}

/// Write the file owner-only (0600) so the token isn't readable by other users.
#[cfg(unix)]
fn write_private(path: &Path, body: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).map_err(|e| e.to_string())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn write_private(path: &Path, body: &str) -> Result<(), String> {
    std::fs::write(path, body).map_err(|e| e.to_string())
}

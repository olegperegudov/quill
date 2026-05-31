//! API-key storage in the OS keychain (macOS Keychain / Windows Credential
//! Manager) instead of a plaintext file.
//!
//! Rationale (CLAUDE.md secrets policy): the key is a real credential, so it
//! lives in the OS secret store, not on disk in cleartext. The rest of the code
//! is unchanged: at startup we load any stored keys into the process env, and
//! `corrector.rs` keeps reading them via `std::env::var(provider.env_var)`.
//!
//! Caveat (documented for the project owner): an ad-hoc-signed Tauri app gets a
//! fresh code signature each release, and the keychain ACL is anchored to it —
//! so after an update macOS may re-prompt once for keychain access. Same class
//! of one-prompt-per-release friction as the TCC reset (see tcc_reset.rs). If
//! that ever becomes annoying, the storage backend is the only thing that has
//! to change.

use crate::corrector::PROVIDERS;
use crate::debug_log;

const SERVICE: &str = "quill";

fn entry(env_var: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, env_var).map_err(|e| e.to_string())
}

/// Pull every stored provider key into the process environment. Called once at
/// startup so the corrector can read keys the usual way.
pub fn load_into_env() {
    for p in PROVIDERS {
        // Respect an already-exported env var (dev override) over the keychain.
        if std::env::var(p.env_var).is_ok() {
            continue;
        }
        if let Ok(e) = entry(p.env_var) {
            if let Ok(key) = e.get_password() {
                if !key.is_empty() {
                    unsafe { std::env::set_var(p.env_var, &key) };
                    debug_log::log(&format!("loaded {} from keychain", p.env_var));
                }
            }
        }
    }
}

/// Store a key in the keychain and make it live in the current process.
pub fn save(env_var: &str, key: &str) -> Result<(), String> {
    entry(env_var)?
        .set_password(key)
        .map_err(|e| e.to_string())?;
    unsafe { std::env::set_var(env_var, key) };
    debug_log::log(&format!("saved {} to keychain", env_var));
    Ok(())
}

/// Whether a usable key for this env var is present (process env or keychain).
pub fn has_key(env_var: &str) -> bool {
    if std::env::var(env_var).map(|k| !k.is_empty()).unwrap_or(false) {
        return true;
    }
    entry(env_var)
        .and_then(|e| e.get_password().map_err(|err| err.to_string()))
        .map(|k| !k.is_empty())
        .unwrap_or(false)
}

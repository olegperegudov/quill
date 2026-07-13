//! The session log: what the app did, never what the user wrote.
//!
//! Quill runs headless in the menu bar, so when a correction lands in the wrong
//! window or a provider quietly fails there is no console to look at — this file
//! is the only witness. Two rules keep it from turning into a transcript:
//!
//! * **Events, not content.** Lines record *that* a correction of N characters
//!   ran against a model, never the characters. The corrected text is the user's
//!   writing — a mail, a message, a password typed into the wrong field — and it
//!   already has a home with an expiry (`logger.rs`, `history_days`). A second,
//!   permanent copy in here would make that expiry a lie.
//! * **Fresh file per launch.** An append-only log on an app that runs for weeks
//!   is a slow disk leak, and only the current session is ever useful.
//!
//! Owner-only (0600) through `private.rs` either way — CopyPaster's log works the
//! same, deliberately.

use chrono::Local;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init() {
    let Some(dir) = dirs::config_dir().map(|d| d.join("quill").join("logs")) else { return };
    let _ = crate::private::create_dir(&dir);
    let path = dir.join("debug.log");
    let _ = crate::private::write(&path, b"");
    if let Ok(mut g) = LOG_PATH.lock() {
        *g = Some(path);
    }
}

pub fn log(msg: &str) {
    let path = match LOG_PATH.lock() {
        Ok(g) => g.clone(),
        Err(_) => return,
    };
    // Before init(), or on a machine with no config dir: fall back to the same
    // file rather than dropping the line — a log that starts late is still a log.
    let path = match path {
        Some(p) => p,
        None => match dirs::config_dir() {
            Some(d) => {
                let dir = d.join("quill").join("logs");
                let _ = crate::private::create_dir(&dir);
                dir.join("debug.log")
            }
            None => return,
        },
    };
    if let Ok(mut file) = crate::private::append(&path) {
        let ts = Local::now().format("%H:%M:%S%.3f");
        let _ = writeln!(file, "[{}] {}", ts, msg);
    }
}

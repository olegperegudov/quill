# Changelog

Engineering release notes. Primary reader: future Claude. Detailed on purpose —
enough to understand *what* changed and *why* without digging through diffs.

## 0.1.x — platform-correct hotkey labels

The hotkey was rendered in Windows form ("ctrl+alt+e") everywhere, including on
macOS, where it should read ⌃⌥E. The stored binding is unchanged (Tauri's
lowercase form — the same physical keys on both OSes); only the *label* is now
platform-aware: glyphs with no separators on macOS (⌃⌥E, ⌘⇧Space), spelled-out
"Ctrl + Alt + E" on Windows. Applied in the window, the live capture display, and
the status line. New `prettyShortcut(raw, isMac)` helper in shortcut.js, unit-
tested both ways. README "How it works" now shows both forms.

## 0.1.3 — one-click platform downloads

The README download buttons now link **straight to the installer** for each
platform instead of dumping you on the Releases page full of every file.

The snag: tauri names assets with the version baked in (`Quill_<ver>_aarch64.dmg`),
and GitHub's stable `releases/latest/download/<name>` redirect needs an exact,
unchanging filename. Fix: CI now re-uploads version-less copies to each release
— `Quill_macOS_AppleSilicon.dmg`, `Quill_macOS_Intel.dmg`, `Quill_Windows_Setup.exe`
(via `gh release upload --clobber` after each build) — and the README buttons
point at those. The versioned files + `.sig` + `latest.json` still ship for the
auto-updater; the stable names are purely for the human download buttons.

## 0.1.2 — real Quill icon

Replaced the inherited Ribbit frog placeholder with Quill's own icon: a white
feather on a violet-ink gradient squircle (matches the app's accent colour).
Source rendered from an SVG; full icon set regenerated via `tauri icon`. Dropped
the iOS/Android icon variants `tauri icon` emits — Quill is desktop-only. Added
`src/quill.png` (256px) for the GitHub profile card. First change to ride the
CI → in-app-update loop end to end.

## 0.1.0 — initial build (forked from Ribbit)

First version. Quill is the text-correction twin of Ribbit (voice-to-text):
same Tauri v2 shell, same hotkey/tray/updater/keychain plumbing, the audio
pipeline swapped for a selection→correct→insert flow.

**What it does.** Global hotkey (default `ctrl+alt+e`) → grab the current
selection → send it to an LLM that fixes spelling/punctuation/grammar in RU or
EN without changing meaning or tone → type the corrected text back over the
selection.

**Kept from Ribbit (unchanged plumbing):** `inserter.rs` (type via `enigo`, no
clipboard paste), `mac_window.rs`, `tcc_reset.rs` (cdhash-rotation permission
re-arm), the tray, the auto-updater, the debug log, and the LLM HTTP client
shape (providers table, retry-once, response parsing) now in `corrector.rs`.

**New / changed:**
- `selection.rs` — the one genuinely new piece. Captures the selection by
  seeding the clipboard with a sentinel, synthesizing the platform Copy chord
  (⌘C / Ctrl+C), polling until the clipboard changes (≈1s ceiling), then
  restoring the original clipboard. Empty after the poll ⇒ nothing was selected.
  We insert by typing, so the clipboard is only ever touched here.
- `corrector.rs` — Ribbit's `postprocess.rs`, retargeted. Dropped the dictation
  vocab. New bilingual system prompt: correct only, never translate, preserve
  tone, return only the text. `max_tokens` now scales with input length so a
  long paragraph isn't truncated (floor 512, cap 8192). Timeout raised 5s→20s
  (a paragraph correction can take a few seconds).
- `secrets.rs` — new. API key in the OS keychain (`keyring`, apple-native /
  windows-native) instead of a plaintext `.env`. Loaded into the process env at
  startup so the corrector reads it the usual way.
- `lib.rs` — rewritten. New `AppState { busy, current_shortcut }`. The hotkey
  fires the flow on **Release** (so the chord's modifiers are up before we
  synthesize ⌘C) with a 60ms settle delay, guarded by `busy` against re-fire.
  Identical-output short-circuit (don't re-type when nothing changed). Config
  dir renamed `ribbit`→`quill`. Dropped audio/transcribe/sound/vocab/usage and
  their deps (cpal, rodio, rusqlite); added arboard + keyring.
- Frontend rewritten as a focused settings window: live status line, model +
  key, click-to-change hotkey, local correction history (click a row to reveal
  the original), update + debug controls. New visual language — dark slate base
  shared with the Ribbit family, system-sans, ink-violet accent.

**Prompt-injection guard.** The selection is arbitrary user content shipped to
an LLM, so the system prompt explicitly tells the model the text is content to
correct, never instructions to obey. Pinned by a unit test.

**Tests.** 15 Rust unit tests (provider table, prompt guarantees + injection
guard, payload shape + max_tokens scaling, response parsing/quote-stripping,
empty/no-key guards) + 4 JS tests for the hotkey-string builder. All green.

**Known follow-ups (not in this version):**
- App/tray icon is still Ribbit's frog placeholder — needs a Quill icon set.
- Updater pubkey in `tauri.conf.json` is a placeholder — a real minisign
  keypair must be generated and its private half added as the CI signing secret
  before the first release.
- Keychain ACL is anchored to the code signature; an ad-hoc-signed build may
  re-prompt for keychain access once per release (same class as the TCC reset).
  If that gets annoying, swap the storage backend in `secrets.rs` — nothing else
  changes.
- Selection capture restores text clipboard contents only (images/files aren't
  preserved across the ~1s borrow).

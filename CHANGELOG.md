# Changelog

Engineering release notes. Primary reader: future Claude. Detailed on purpose —
enough to understand *what* changed and *why* without digging through diffs.

## 0.1.12 — search, Ribbit-style updates, and minimalist polish

Front-end only again; same data, same Rust. Brings the main window the rest of
the way to Ribbit's minimalism.

- **Search (magnifier in the header).** A live filter popup, like Ribbit — but
  it matches the *original* text too, not just the corrected result. A row stays
  if the query hits either side; matches are highlighted (`<mark>`). When the hit
  is only in the original (hidden under the clock), the **clock lights up** so you
  know to hold it — and holding reveals the original with the match highlighted.
  Substring match, case-insensitive. `entryMatchesQuery`/`highlightInto`/
  `applySearch`/`renderRowText` in main.js; `#search-btn` + `#search-popup`.
- **Updates work exactly like Ribbit now.** The standalone footer button is gone
  (the whole footer is); the update control lives in settings, and when a release
  is found the **gear glows green** so you spot it from the log without opening
  settings. Inside, the button shows `update to vX`, then `downloading N%` on the
  button itself, then the app restarts. `setupUpdates` rewritten as a clean
  swap-the-onclick state machine (check ⇄ install) to avoid Ribbit's
  double-handler quirk; `.update-available` glow on both `#update-btn` and
  `#settings-btn`.
- **Minimalist header.** Dropped the persistent "Ready" — the status subtitle is
  empty (and hidden) at rest, surfacing only while a correction or download is in
  flight, then clearing. Just "Quill" the rest of the time.
- **Empty state actually centers.** It used to share the column with the
  (empty) list and drift below middle; the list is hidden when there are no
  entries, so the welcome sits dead-centre.

## 0.1.11 — main window: a chat-style log + settings behind a gear

Front-end only; the correction flow and the whole Rust side are untouched. The
main window used to greet you with the model/key card and a hotkey hint up top
and the history below — settings shouting before you'd done anything. **Now** it
opens like Ribbit's log: a clean chat-style list of past corrections (newest on
top), and settings tuck behind a gear in the header.

- **Settings behind the gear.** New `⚙` button in the titlebar toggles between
  the log (`#log-view`) and a `#settings-panel` that holds Hotkey, Model, API
  key and the debug-log opener (Ribbit-style label-left / control-right rows).
  When there's no API key yet, startup auto-opens settings so onboarding still
  works. View switch is a plain show/hide (`showView`), debug stays an overlay.
- **Status moved to a header subtitle.** The standalone status pill is gone;
  the live state ("Ready / working / done / error") is now a quiet line under
  the wordmark, colour-coded, settling back to "Ready". Keeps the body clean.
- **Log rows redesigned (`logRow`).** Each finished correction is a flat row —
  time, the polished text, and on the right a **clock** you *press and hold* to
  reveal the original (dimmed + italic), releasing to snap back to the corrected
  text. Pointer-capture on press so the release restores even if the cursor
  drifts off the button. Unchanged corrections show "already clean" instead of a
  clock (nothing to peek at). Replaces the old click-anywhere-to-toggle card.
- **Empty state.** A centred "Nothing yet" + the hotkey hint, shown when the log
  is empty — the clean welcome, no settings in sight.
- Data is unchanged: `get_log_history` / the `correction` event / `logger.rs`
  per-day JSONL store all stay as-is; only the rendering changed.

## 0.1.10 — editor window: review before it lands (Grammarly-style redesign, phase 1)

The big UX shift. **Was:** the hotkey silently replaced the selection with the
corrected text — no feedback, no chance to read what changed or tweak it before
it landed. **Now:** the hotkey captures the selection and opens a dedicated Quill
editor window over it; the window runs the correction itself, shows the result
for you to read and hand-edit, and on **Apply** it re-activates the app you were
in and types the final text back over the (still-present) selection. **Cancel /
Esc** types nothing — the original is left untouched.

Mechanics worth knowing for the next change:
- New `editor` webview window (label `editor`, hidden until the hotkey fires;
  preloaded at startup so its event listener is live). Shares the `default`
  capability with `main` (added `editor` to the capability's window list) so it
  can invoke commands and listen for events.
- `mac_focus.rs` (new, macOS): grabs the frontmost app's pid via
  `NSWorkspace.frontmostApplication` at capture time — *before* our window steals
  focus — and re-activates it (`NSRunningApplication.activateWithOptions`) just
  before typing. This is the load-bearing new risk: showing a window means the
  target app loses focus, so the type-back now depends on returning it. Off-macOS
  it's a no-op (hiding our window already restores focus there).
- `lib.rs`: the hotkey no longer corrects+inserts; it captures → remembers the
  front app → shows the editor → emits `editor:open` with the text. New commands:
  `editor_correct` (async + `spawn_blocking` so the editor UI keeps animating
  during the round-trip), `apply_correction` (logs history, hides editor,
  re-activates the target, types), `close_editor` (cancel). The tray "working"
  glyph was dropped — the editor window is now the feedback surface.
- Front end: `editor.html/.css/.js` — a textarea over the captured text with a
  status line and Apply/Recheck/Cancel. ⌘⏎ applies, Esc cancels. A `reqId`
  stale-guard already gates the correction (load-bearing once live re-checking
  lands in a later phase). Styling mirrors the settings window's tokens.

Still to come (later phases): live per-word underlines on what changed,
click-a-word to see было→стало and accept/reject, select-a-chunk to rewrite, and
debounced re-checking as you type. This phase is the window + focus-return
foundation only.

Verification: compiles clean, the 16 unit tests stay green, and the editor
window was eyeballed via a headless render (matches Quill's look). The
focus-return + type-back across real apps (Telegram, browser, Mail) is the one
path that needs this live release to confirm — it can't be exercised from a
headless build.

## 0.1.9 — menu-bar "working" indicator

The settings window lives in the tray, so when you trigger a correction from
another app (Telegram, a browser) there was no on-screen sign anything was
happening during the ~3s LLM round-trip — it read as "nothing happens / broken".
Now the menu-bar tray shows a "…" while a correction is in flight and clears when
done. (Verified end-to-end that the correction itself works — incl. in Telegram;
the menu-bar glyph rendering couldn't be eyeballed from the build environment.)

## 0.1.8 — update progress feedback + flow logging

- The update button gave no feedback during the 20-30s download — the click felt
  dead. It now shows live progress ("downloading 45%") and mirrors it in the
  status line, driven by the `update-progress` events the Rust side already
  emitted but the UI ignored (mirrors Ribbit). Also collapsed the update click
  logic into a single handler (no more stray `onclick` double-firing with the
  `addEventListener` one).
- Instrumented the correction flow: logs "hotkey fired → capturing" and
  "captured N chars" so a silent no-op is diagnosable from the debug log instead
  of leaving no trace. (Verified the full select→correct→insert path end-to-end
  on macOS via a synthetic hotkey against TextEdit — works; the engine, capture,
  and insert are all fine.)

## 0.1.7 — fix crash when triggering a correction (macOS)

**What:** pressing the hotkey instantly crashed Quill on macOS (SIGTRAP).

**Was:** selection capture synthesized ⌘C with `enigo.key(Key::Unicode('c'))`.
On macOS that makes enigo resolve the keycode through the Text Input Source APIs
(TSM / HIToolbox), which `dispatch_assert_queue` the **main thread** and abort the
process when called from our worker thread — and the whole correction flow runs
on a worker thread. The ⌘ modifier was fine (fixed keycode); only the
layout-dependent `'c'` lookup tripped the assert. (`enigo.text()`, used to type
the result, takes the CGEvent Unicode path and is safe off-main — which is why
Ribbit, which only ever types, never hit this.)

**Now:** on macOS we send the raw keycode of the physical C key
(`Key::Other(0x08)` = kVK_ANSI_C), which skips the TSM lookup — no main-thread
requirement, no crash. Bonus: ⌘C now fires regardless of the active keyboard
layout (e.g. a Cyrillic layout), which suits a bilingual tool. Windows keeps
`Key::Unicode('c')` (no TSM there).

**Tests:** a guard test pins the macOS copy key as a raw keycode (never
`Key::Unicode`), so this crash class can't quietly return.

## 0.1.6 — platform-correct hotkey labels

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

# Changelog

Engineering release notes. Primary reader: future Claude. Detailed on purpose —
enough to understand *what* changed and *why* without digging through diffs.

## 0.1.21 — stable self-signed signing (Accessibility grant survives updates), normal window

**Three reports.** (1) The window floated above every other window — couldn't be
sent behind. (2) After an update the selection wasn't captured: the chat opened
empty and the user had to paste by hand. (3) The VPN still dropped on the 0.1.20
update.

**0.1.20's keychain theory was wrong — corrected here.** The debug log after the
0.1.20 update shows *no* `loaded … from keychain` line (the new file-based build
never touched the keychain) yet the VPN dropped anyway → keychain wasn't the
cause. `tccutil` is ruled out too: Ribbit's `tcc_reset.rs` is byte-identical and
its log resets TCC on every update, but Ribbit's VPN never drops. So both prior
suspects are eliminated by evidence. (Keychain→file from 0.1.20 stays — it does
kill the post-update password prompt — it just wasn't the VPN trigger.)

**Real root cause (capture + the VPN's last lead): ad-hoc signing.** Each ad-hoc
release gets a fresh cdhash, and a TCC grant's designated requirement pins that
cdhash. So after every update the Accessibility grant goes stale: System Settings
still shows Quill "allowed", `AXIsProcessTrusted()` returns true, but the
synthetic ⌘C is filtered at kCGHIDEventTap → `captured 0 chars`. The debug log
(is_trusted true, 0 chars) and the screenshot (Quill toggled on) confirm it. This
is also the **only** remaining Quill-vs-Ribbit difference for the VPN: Quill is
the one that needs the most-privileged grant (Accessibility) re-issued every
update; Ribbit only needs Microphone.

**The fix — stable self-signed certificate.** CI now signs both macOS arches with
a self-signed code-signing cert (secrets `APPLE_CERTIFICATE` /
`APPLE_CERTIFICATE_PASSWORD` / `APPLE_SIGNING_IDENTITY`, imported by
tauri-action). The designated requirement then anchors to the **certificate**
(`identifier "com.quill.app" and certificate root = H"…"`), not the cdhash — so
the Accessibility grant survives every update and the selection capture keeps
working. Not notarized (no Apple account), so first launch still needs a
right-click→Open, exactly like the old ad-hoc builds.

- `tcc_reset.rs` rekeyed off the **signing identity** instead of the cdhash: it
  resets once on the ad-hoc→cert migration (clears the stale ad-hoc grant so the
  user gets a clean prompt), then never again across cert-signed builds. Keying
  off cdhash would have wiped the good grant on every release.
- `selection.rs`: a 20 ms settle on each side of the ⌘ chord, so the OS doesn't
  see a bare "c" (copy never fires) in fussy apps — terminals, Electron.
- Window: `alwaysOnTop` false (was true). It opens at the cursor and takes focus
  on the hotkey, but no longer floats above other windows. Matches Ribbit.
- UI is now all English (was mixed RU/EN): every label, placeholder, tooltip,
  status line, and the one user-facing Rust error. Dropped the "history is local
  only" settings note. Russian stays only in corrector.rs test fixtures (they
  exercise the RU correction path) and in user-entered chat content.

One-time: the transition update re-grants Accessibility once (the old ad-hoc
grant doesn't match the cert-signed binary); from then on it persists. Whether
the VPN survives is verified on the update *after* this one (the first clean,
cert-to-cert update).

## 0.1.20 — store the API key in a file, not the macOS Keychain

**The symptom.** Every Quill update (a) re-prompted for the macOS login password
and (b) dropped the user's corporate VPN (the 2FA-push one). A plain relaunch of
the *same* binary did neither; running `tccutil reset All com.quill.app` by hand
did neither. So the trigger was something Quill does **only on update** that
Ribbit — same updater, same ad-hoc per-arch signing, VPN never drops — does not.

**The difference, found by diffing the two apps.** Build/signing/Info.plist/
updater/restart are byte-identical between Quill and Ribbit (so stable signing
was a dead end — Ribbit isn't stably signed either). The one functional
divergence: **Quill kept the API key in the macOS Keychain (`keyring` crate);
Ribbit keeps it in a config file.** An ad-hoc binary's signature rotates each
release, and a Keychain ACL is anchored to that signature — so the first launch
after an update hits an ACL **mismatch** and macOS does a heavier "signature
changed, re-authorize" pass on the login keychain. That lines up exactly with
both the password prompt and the VPN (whose 2FA session lives in the same login
keychain) dropping on every update.

**The fix (Ribbit's approach).** Store the key in `config_dir/quill/.env`,
written `0600`. Dropped the `keyring` dependency; `secrets.rs` now reads/writes
that file with the same public API (`load_into_env` / `save` / `has_key`), so
lib.rs callers are unchanged. An API key can't be hashed (it's sent to the
provider verbatim), so the realistic choice is keychain-vs-file; the file is the
user's own credential on their own machine, owner-only. Quill never touches the
keychain now.

One-time: the existing key sits in the old Keychain entry the new build no longer
reads, so the key must be re-entered once after updating. Also fixed a stale
capability referencing the removed `main` window.

Confirmation pending: the password prompt is gone for certain; whether the VPN
now survives an update is verified on the next update.

## 0.1.19 — gear always reachable: titlebar stays, the body swaps views

**What was wrong.** 0.1.18 made settings a full-cover overlay (`position:absolute;
inset:0`), which painted over the titlebar too — so once in settings the gear was
gone and there was no way to flip back to the chat/log. (Also two minor CI/build
follow-ups landed as 0.1.18.x.)

**The fix — Ribbit's real structure.** The titlebar is now persistent; only the
body below it swaps between three views:
- `setView("chat"|"settings"|"debug")` toggles `#log`+`#composer` vs
  `#settings-panel` vs `#debug-panel` (each a `.view-panel` flex child, not an
  overlay). The gear lives in the always-visible titlebar, so it flips chat ↔
  settings from either side; it shows an `.active` tint while in settings.
- **Status moved into the titlebar** (under the wordmark), so "Ключ сохранён" /
  "Hotkey: …" is visible from any view.
- Debug log is a third view reached from settings (`>_`) with its own back; Esc
  peels debug → settings → chat → hide window. Capture still owns Esc via
  `.capturing`.
- View switching consolidated in editor.js; settings.js no longer touches the
  debug panel. No second window, no overlay covering the chrome.

Verified headless: the gear is visible in chat, settings, AND debug; the log
hides in settings; the gear toggles back to chat; Esc peels each layer. `vitest`
7/7, no console errors.

## 0.1.18 — one window: the gear flips settings over the chat, no second window

**What was wrong.** 0.1.17 made the chat the app's face but kept settings as a
*separate* window — so clicking the gear opened a second window. The user wanted
one window that switches between the chat (text + history) and settings, exactly
like Ribbit (its gear swaps the log view for a settings view in place).

**The fix — settings is an in-window overlay.**
- The gear now flips a `#settings-panel` overlay over the chat (and a `>_` debug
  overlay above that), the same way the chat already overlaid its debug log.
  `showView`-style toggling, one window — mirrors Ribbit's gear behaviour.
- **Esc peels back one layer**: debug → settings → hide the window. While a
  shortcut capture is live, settings.js owns Esc (cancels capture) — editor.js
  defers via the `.capturing` class on the kbd.
- The settings **window is gone**. Removed the `main` window from
  `tauri.conf.json`; the chat (`editor`) is the only window, sized 420×580 to
  fit both views. Rounded-corners / Spaces polish now applies to it alone.
- **Frontend consolidated.** Settings logic moved out of the deleted
  `main.js`/`index.html`/`styles.css` into a new `settings.js` module
  (`initSettings()`), imported by `editor.js`; its styles ported into
  `editor.css` as `.panel` overlays. `shortcut.js` is shared by both.
- **Backend trimmed.** Removed the now-dead commands `show_main_window`,
  `hide_to_tray`, `show_from_tray`, `set_always_on_top` and the `show-settings`
  event. First-run onboarding reveals the chat window; editor.js sees the
  missing API key and opens the settings overlay itself (no cross-window event).

Verified: `cargo check` clean (only pre-existing cocoa deprecation warnings);
`vitest` 7/7 green; headless render of both views (chat with day-separated
history + settings overlay) — one window, gear toggles, Esc peels layers, no
console errors.

## 0.1.17 — the chat is the only face; the old clock-log window is gone

**The regression it fixes.** Every update restarts the app, and on launch the
*settings* window (which still carried the old clock-rewind history list) popped
up on its own. So after updating, the user saw the thing we'd replaced — a log
with clock tongues, no composer, no chat — and reasonably thought the chat was
gone. The chat was fine; it was just a second window that only opened on the
hotkey, while the wrong window greeted them on launch.

**The fix — collapse to one face.**
- The **chat is the app**. The tray icon (click + "Show" menu) now toggles the
  chat window, not settings. The hotkey already opened the chat.
- **Nothing pops on launch.** Both windows are `visible: false`; Quill lives in
  the tray. The sole exception is genuine first-run with no API key, which opens
  settings so the hotkey isn't a dead end.
- The **settings window is settings only**. Removed the clock-rewind history
  list and the search box from it (and their dead CSS) — there are no clock
  tongues anywhere now. It opens only from the chat's gear (`show_main_window`).
- Tray retarget: `toggle_main_window` → `toggle_chat_window` (targets the editor
  window); launch-time onboarding check added in `setup`.

History still lives in the chat (loaded on open, day-separated). Settings keeps
model / key / hotkey / update / debug.

## 0.1.16 — blank when empty; day separators in the chat (Ribbit parity)

- **Empty chat is empty.** Dropped the "select text and press ⌃⌥E" greeting.
  First open with no history shows nothing — like Ribbit's log.
- **Day separators.** Between calendar days the chat now draws a thin rule with
  a small `we, jun 25th` label (weekday + month + ordinal day). Ported verbatim
  from Ribbit's `formatDate` + `.date-sep`, placed in chat order (a day's
  separator heads its first message, oldest day at top).
- Retention was already Ribbit's exact mechanism (shared `logger.rs`: a rolling
  window of day-files, today + the previous N-1 days, default 7) — left as is.
- Select-text → hotkey → it lands in the chat and is corrected automatically was
  already in place (the `editor:capture` path); no copy/paste needed.

## 0.1.15 — update progress in one place

`downloading NN%` was shown both on the update button and in the header
subtitle. Now only on the button (Ribbit-style); update failures land there too,
then re-arm for retry.

## 0.1.14 — the hotkey opens a chat at the cursor; copy instead of type-back

The editor popup becomes a chat, and the two things that still felt broken
after 0.1.13 are fixed.

**What was wrong.**
- The popup opened on whichever Space it last lived on. The user pressed the
  hotkey, saw nothing, and only later found the window on another desktop.
- The 0.1.13 "grant Accessibility" screen was a half-screen overlay that
  couldn't be moved or closed — worse than the problem. The macOS system
  prompt already does the job.
- Typing the corrected text back over the selection was the fragile half of
  the Accessibility story. Copying needs none of that reach.

**The redesign.**
- `editor.*` is now a chat. The captured selection lands as your bubble (accent,
  right); the correction replies below it (panel, left). A composer at the
  bottom takes typed/pasted text — Enter sends, Shift+Enter newlines — through
  the same correct→reply path, so re-polishing is: click your bubble (copies),
  paste, tweak, Enter. **Click any bubble to copy it** to the clipboard; you
  paste it yourself. No more type-back, no clock-to-rewind tongues.
- **Window opens at the mouse cursor**, clamped to the cursor's screen, on the
  active Space. New `mac_window::position_at_cursor` (`NSEvent.mouseLocation` +
  `NSScreen.visibleFrame`), run on the main thread before `show()`.
- **No in-app permission overlay.** When Accessibility isn't granted the hotkey
  pops the real macOS dialog and shows the chat with one quiet inline note
  (`editor:need-access`).
- Dropped the type-back path: removed `inserter.rs`, `mac_focus.rs`,
  `apply_correction`, and `AppState::target_pid`. Added `copy_to_clipboard`
  (arboard) and `show_main_window` (the gear → settings, landing on the
  settings view via `show-settings`).
- `editor_correct` now records the original→corrected pair in history itself
  (unchanged "already clean" text isn't logged). History loads into the chat
  oldest-at-top on open.
- The chat's gear glows green when an update is waiting; the install button
  still lives in settings.

Capture still uses the synthetic ⌘C, so reading the selection needs
Accessibility — but that is now the *only* thing that does.

## 0.1.13 — the hotkey never dies silently; ask for Accessibility out loud

The recurring "Quill doesn't work after an update" finally fixed at the root.

**The bug.** Each release rotates the ad-hoc cdhash, so `tcc_reset` wipes the
Accessibility grant — and then the hotkey would capture the selection with a
synthetic ⌘C that macOS silently filters (no permission), get nothing, and
*open no window at all*. From the user's seat: press the hotkey, nothing
happens, no hint why. The old code assumed macOS would re-prompt on the next
synthetic keystroke; it doesn't — posting an event without the grant just fails
quietly, it never triggers a prompt.

**The fix.**
- New `accessibility.rs`: `is_trusted()` (`AXIsProcessTrusted`) and `prompt()`
  (`AXIsProcessTrustedWithOptions` with the prompt option) — the latter pops the
  real macOS "allow Quill to control this computer" dialog. Needs the
  `core-foundation` crate + the ApplicationServices framework.
- `launch_editor` rewritten so **the window always opens**. If we're not trusted
  it pops the macOS dialog and shows the editor on a "grant access" screen
  instead of attempting a doomed silent ⌘C. If we are trusted it captures and
  opens as before — and now even an *empty* capture opens the window (type/paste)
  rather than dying.
- Editor gains a permission screen (`editor:permission` event): explains it needs
  Accessibility, with **Open System Settings** (`open_accessibility_settings`)
  and **I've enabled it → retry** (`accessibility_status`) buttons.

After an update the flow is now Ribbit-like: press the hotkey → macOS asks for
the grant → enable Quill → it works. No more silent dead key press.

Still ad-hoc signed, so the grant doesn't *survive* updates yet — a stable
signing identity (so the cdhash stops rotating) is the next step. But the app no
longer hides the problem.

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

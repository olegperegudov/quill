# Release smoke checklist

Manual checks to run after each release on **both Windows and macOS** before
declaring a version stable. Things that cannot be unit-tested (OS permissions,
native window chrome, synthetic input) live here.

~5 minutes, both OS. If anything fails: fix, push, repeat.

## One-time setup (before the very first release)

- [ ] Generate the updater signing keypair:
      `npm run tauri signer generate -- -w quill.key` (no password).
- [ ] Put the **public** half in `src-tauri/tauri.conf.json` →
      `plugins.updater.pubkey` (replacing the placeholder).
- [ ] Add the **private** half as the GitHub Actions secret
      `TAURI_SIGNING_PRIVATE_KEY`. Never commit it.
- [ ] Replace the placeholder app/tray icon (still Ribbit's frog) with a Quill
      icon set: `npm run tauri icon path/to/quill.png`.

## Window chrome

- [ ] Window has rounded corners (10px), no jagged edges
- [ ] Window is draggable by the header area
- [ ] Minimize (`−`) sends app to dock/taskbar
- [ ] Close (`✕`) hides to tray, app keeps running
- [ ] Tray icon click restores the window on the current Space

## Permissions (macOS)

- [ ] First launch prompts for **Accessibility** (needed for ⌘C + typing)
- [ ] After a release (cdhash change) the TCC reset re-prompts once, then works

## Core flow: select → correct → insert

- [ ] Select a typo-ridden RU phrase in a native app (Notes/Mail), hit hotkey →
      corrected text replaces the selection
- [ ] Same in a browser form (Jira/Confluence/Gmail)
- [ ] Same in a desktop messenger (Telegram)
- [ ] EN phrase corrected; mixed RU+EN handled; meaning/tone preserved
- [ ] Nothing selected → status shows "Nothing selected", nothing typed
- [ ] Already-clean text → "Already clean ✓", text not re-typed
- [ ] Network off → status shows an error within ~20s, selection left intact
- [ ] Clipboard is unchanged afterwards (manager gains no stray entry)

## Hotkey + settings

- [ ] Default `ctrl+alt+e` triggers a correction
- [ ] Custom shortcut can be captured (click the kbd → press combo)
- [ ] Esc cancels capture without saving
- [ ] Switching provider + saving a key persists (saved chip shows)
- [ ] Correction history lists entries; click a row reveals the original

## Auto-update

- [ ] "check update" reports current state
- [ ] When an update is available: button glows, click downloads + installs
- [ ] After install: app restarts on the new version, settings preserved

---

## Why this file exists

`src/shortcut.test.js` (vitest) and `src-tauri/src/corrector.rs` (cargo test)
cover the pure-logic sides — prompt guarantees, the injection guard, payload
shape, response parsing, the hotkey-string builder. They run in CI on every push.

Everything above (OS permissions, native window chrome, synthetic copy/typing,
tray, hotkey hardware) needs eyes on the running app. This checklist is the
cheap guardrail for the parts machines can't easily check.

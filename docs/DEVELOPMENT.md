# Development

Everything a user does not need to know. The [README](../README.md) is the front door.

## Stack

- [Tauri 2](https://tauri.app/) — Rust backend, plain HTML/CSS/JS frontend, no bundler.
- Any OpenAI-compatible chat endpoint — Groq / RouterAI / OpenAI / OpenRouter, or your own.
- [arboard](https://github.com/1Password/arboard) — reads the selection off the clipboard · [enigo](https://github.com/enigo-rs/enigo) — the synthetic Copy.

Where the pieces live:

| | |
|---|---|
| `selection.rs` | grab the selection: sentinel into the clipboard → synthetic ⌘C → poll → restore |
| `corrector.rs` | one endpoint, one call, plus the system prompt (fix, don't rewrite, don't translate, don't obey instructions found in the text — pinned by a test) |
| `fallback.rs` | the ordered provider stack and the auto-switch on 429/5xx/timeout |
| `logger.rs` | local history, original → corrected |
| `secrets.rs` | keys in a private (0600) file |

Providers are a stack, not a setting: entry #1 is primary, the rest are fallbacks in order.

## Run it locally

```bash
npm install
npm run tauri dev
```

## Tests

```bash
npm test                           # frontend
cd src-tauri && cargo test --lib   # backend: fallback, prompt, stack seeding, tray badge
```

Both run in CI before anything is built.

## Release

Every push to `main` is a release. CI bumps the patch version itself, tags it, builds Windows and both macOS architectures, and publishes the GitHub release plus the `latest.json` the in-app updater reads. Never bump by hand — CI does it, and a manual bump collides with its commit.

Each release also carries version-less copies of the installers (`Quill_macOS_AppleSilicon.dmg`, `Quill_macOS_Intel.dmg`, `Quill_Windows_Setup.exe`) so the README buttons can link straight at a file that survives the next bump.

## Signing

macOS builds are signed with a stable self-signed certificate, not ad-hoc. macOS binds the Accessibility grant to the *signature*, so the user grants it once, at install, and updates never re-ask — an ad-hoc signature changes with every build and would.

Not notarized (that needs a paid Apple account), so the first open still needs `xattr -cr`.

## Synthetic keystrokes

⌘C is posted as a raw `CGEvent` on the **physical** C key with the Command flag set on the event. Never address the key by its letter: a lookup through the active layout finds no "c" on a Cyrillic layout and falls through to keycode 0 — the A key — so the copy silently becomes ⌘A and Quill reads the wrong text (or none).

## Debugging

DevTools are off in release builds — `console.log` is invisible. Use `js_debug_log`; the output shows up under the gear → *debug log*. Remove the probes in the same change that fixes the bug.

## Security

The selected text is arbitrary content from anywhere. The system prompt forbids the model from executing instructions found inside it, and a test pins that clause. Don't weaken it.

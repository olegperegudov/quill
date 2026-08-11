# Agent notes

Quill corrects selected text in place: a hotkey grabs the selection, an
OpenAI-compatible model fixes spelling, punctuation and grammar, the result is
typed back. Tauri 2: Rust does the capture, the network call and the insert;
the webview is the window you see. No frontend framework, no bundler — `src/`
is served as-is.

Start here: **[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)** — layout, window
behaviour, providers, signing, CI.

Quick facts:

- Run it: `npm install && npm run tauri dev`
- Tests: `npm test` (vitest) and `cargo test --lib` in `src-tauri/`
- The API key lives in a `0600` file written through `private.rs`; it is never
  logged, never sent to the frontend and never leaves the machine except to the
  endpoint the user configured (https only).
- The correction prompt forbids the model to follow instructions found inside
  the user's text — that guard is pinned by a test, do not loosen it.
- Versions are bumped by CI on push to `main` — do not edit the version in
  `src-tauri/tauri.conf.json` or `Cargo.toml` by hand.
- User-visible changes go in `CHANGELOG.md` under `## Unreleased`, one plain
  bullet per change; CI cuts that section into the release notes.

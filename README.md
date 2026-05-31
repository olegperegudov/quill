# Quill ✒

Polish your writing in place. Select text in **any** app — chat, email, Jira,
a browser form — press a global hotkey, and Quill quietly fixes spelling,
punctuation and grammar (Russian **and** English) without changing your meaning
or tone, typing the corrected text back over your selection.

No window to switch to, no copy-paste. The hotkey is the whole interface.

## How it works

1. You select text and press the hotkey (default `⌃⌥E`).
2. Quill copies the selection, sends it to an LLM with a strict "correct, don't
   rewrite" prompt, and types the result back over the selection.
3. Your clipboard is borrowed for a fraction of a second during capture and put
   back exactly as it was — Quill inserts by typing, never by pasting.

Language is detected automatically; a casual message stays casual. If nothing is
selected, or the model would change nothing, Quill does nothing.

## Setup

1. Open Quill (it lives in the menu-bar / tray).
2. Pick a model provider and paste its API key — stored in the OS keychain.
3. Select some text anywhere and hit the hotkey.

On macOS, grant **Accessibility** when prompted — Quill needs it to read the
selection and type the correction.

## Privacy

Selected text is sent to the model provider you configure (RouterAI by default).
A local, on-device history of corrections is kept so you can see what changed;
its retention is configurable and it never leaves your machine.

## Development

A Tauri v2 app (Rust core + a small HTML/JS settings window), forked from
[Ribbit](https://github.com/olegperegudov/ribbit). Verification is via the
in-app updater after a CI release — see [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md).
Engineering notes live in [CHANGELOG.md](CHANGELOG.md).

```
src-tauri/src/
  lib.rs        hotkey → capture → correct → insert, tray, updater
  selection.rs  grab the current selection (synthetic Copy + clipboard)
  corrector.rs  call the LLM, return corrected text (the prompt lives here)
  inserter.rs   type the result over the selection
  secrets.rs    API key in the OS keychain
src/            settings window (index.html / main.js / styles.css)
```

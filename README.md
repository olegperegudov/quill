<p align="center">
  <img src="src/quill.png" width="96" alt="Quill logo" />
</p>

<h1 align="center">Quill ✒</h1>

<p align="center">
  Polish your writing in place, in any app, on Windows and macOS.<br/>
  Select text, press a hotkey — spelling, punctuation and grammar fixed, your meaning untouched.
</p>

<p align="center">
  <i>Highlight the mess, then tap the key —<br/>
  Quill tidies the typos quietly.<br/>
  Your tone, your words, your meaning stay,<br/>
  just the slips and commas swept away.</i> ✒
</p>

<p align="center">
  <b>Russian and English</b> — language detected automatically, never translated<br/>
  <b>Private by design</b> — text goes only to your chosen model, history stays local, no telemetry
</p>

## How it works

1. **Select** text in any app — a chat, an email, Jira, a browser form
2. Press **Ctrl+Alt+E** (customizable)
3. Quill sends the selection to an LLM that fixes spelling, punctuation and grammar — without changing your meaning or tone
4. The corrected text is typed back **over your selection**

No window to switch to, no copy-paste. If nothing is selected, or the text is already clean, Quill does nothing. Your clipboard is borrowed for a split second to read the selection, then restored exactly as it was.

## Quick start

### 1. Download

<p>
  <a href="https://github.com/olegperegudov/quill/releases/latest"><img src="https://img.shields.io/badge/Windows-0078D4?style=for-the-badge&logo=windows&logoColor=white" alt="Download for Windows" /></a>&nbsp;
  <a href="https://github.com/olegperegudov/quill/releases/latest"><img src="https://img.shields.io/badge/macOS_%E2%80%93_Apple_Silicon-000?style=for-the-badge&logo=apple&logoColor=white" alt="Download for macOS Apple Silicon" /></a>&nbsp;
  <a href="https://github.com/olegperegudov/quill/releases/latest"><img src="https://img.shields.io/badge/macOS_%E2%80%93_Intel-666?style=for-the-badge&logo=apple&logoColor=white" alt="Download for macOS Intel" /></a>
</p>

Grab the installer for your platform from the [Releases](https://github.com/olegperegudov/quill/releases/latest) page:

- **Windows** — `Quill_x.y.z_x64-setup.exe`
- **macOS (Apple Silicon)** — `Quill_x.y.z_aarch64.dmg`
- **macOS (Intel)** — `Quill_x.y.z_x64.dmg`

On macOS the app isn't notarized yet, so the first launch is **right-click → Open** (then **Open** again) to get past Gatekeeper. You'll also be asked to grant **Accessibility** — Quill needs it to read your selection and type the correction.

### 2. Get an API key

Quill talks to any OpenAI-compatible model. Pick one provider and get its key:

- [**RouterAI**](https://routerai.ru) (default) · [**OpenAI**](https://platform.openai.com/api-keys) · [**OpenRouter**](https://openrouter.ai/keys)

Grammar fixes are tiny requests, so this costs pennies.

### 3. Paste the key into Quill

Open Quill (it lives in the menu-bar / tray), pick your provider, and paste the key. It's stored in your OS keychain. Done — select some text anywhere and hit the hotkey.

## Features

- **Fix in place** — highlight text in any app, press the hotkey, corrected text replaces it
- **Bilingual** — Russian and English, detected automatically, never translated
- **Keeps your voice** — fixes errors, never rewrites your meaning, tone or register
- **Works everywhere** — global hotkey from any app (desktop messengers, browser, mail), even when minimized to tray
- **Leaves your clipboard alone** — inserts by typing; the clipboard is only read, then restored
- **Local history** — see exactly what changed (original → corrected); retention is configurable
- **Key in the OS keychain** — not a plaintext file
- **System tray** — runs quietly in the background, X hides to tray
- **Auto-update** — checks on its own, one-click install from the window
- **Customizable hotkey** — click the hotkey, press your combo

## Privacy

- Selected text is sent to **your chosen provider** for correction only — nothing else leaves your machine
- Your API key is stored in the **OS keychain** (macOS Keychain / Windows Credential Manager)
- Correction history is stored locally and pruned to your retention window; it never leaves your machine
- No analytics, no tracking, no telemetry — the only network call is to the model you picked
- Fully open source — inspect every line

## Settings

| Setting | Description |
|---------|-------------|
| **Model** | Choose your provider (RouterAI / OpenAI / OpenRouter) |
| **API key** | Paste your provider key — stored in the OS keychain |
| **Hotkey** | Click to customize, press your combo |
| **Debug log** | View internal logs for troubleshooting |
| **Version** | Click to view the changelog |
| **Check update** | Manually check for a new version |

## Tech stack

- [Tauri 2](https://tauri.app/) — Rust backend, HTML/CSS/JS frontend
- OpenAI-compatible chat completions — RouterAI / OpenAI / OpenRouter
- [Enigo](https://github.com/enigo-rs/enigo) — synthetic keyboard input (copy + type)
- [arboard](https://github.com/1Password/arboard) — read the selection from the clipboard
- [keyring](https://github.com/hwchen/keyring-rs) — API key in the OS keychain

## Building from source

```bash
# Prerequisites: Node.js, Rust toolchain
npm install
npm run tauri build
```

The installer lands in `src-tauri/target/release/bundle/`.

## License

MIT

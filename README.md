<p align="center">
  <img src="src/quill.png" width="96" alt="Quill logo" />
</p>

<h1 align="center">Quill</h1>

<p align="center">
  Clean up your writing without leaving the window you're in.<br/>
  Select the text, press a hotkey — spelling, punctuation and grammar fixed, your meaning untouched.
</p>

<p align="center">
  <b>Russian and English</b> — the language is detected, never translated<br/>
  <b>Yours</b> — the text goes only to the model you picked, history stays local, no telemetry
</p>

## Get it

<p align="center">
  <a href="https://github.com/olegperegudov/quill/releases/latest/download/Quill_macOS_AppleSilicon.dmg"><img src="https://img.shields.io/badge/Download_for_macOS-Apple_Silicon-000?style=for-the-badge&logo=apple&logoColor=white" alt="Download for macOS, Apple Silicon" /></a>&nbsp;
  <a href="https://github.com/olegperegudov/quill/releases/latest/download/Quill_macOS_Intel.dmg"><img src="https://img.shields.io/badge/Download_for_macOS-Intel-666?style=for-the-badge&logo=apple&logoColor=white" alt="Download for macOS, Intel" /></a>&nbsp;
  <a href="https://github.com/olegperegudov/quill/releases/latest/download/Quill_Windows_Setup.exe"><img src="https://img.shields.io/badge/Download_for-Windows-0078D4?style=for-the-badge&logo=windows&logoColor=white" alt="Download for Windows" /></a>
</p>

Each button downloads the latest installer for that platform. Want an older build? Every version is on the [releases page](https://github.com/olegperegudov/quill/releases).

Then:

1. **Open it.** Apple isn't paid to trust us, so the first launch claims the app is *"damaged"*. It isn't — run `xattr -cr /Applications/Quill.app` once in Terminal, then open it normally. Updates after that install themselves. You'll also be asked for **Accessibility**, once: that's how Quill reads what you selected.
2. **Paste a key.** Any OpenAI-compatible model will do — [Groq](https://console.groq.com/keys) (the default, and fast), [RouterAI](https://routerai.ru), [OpenAI](https://platform.openai.com/api-keys), [OpenRouter](https://openrouter.ai/keys). A correction is a tiny request; this costs pennies.
3. **Select text anywhere, press ⌃⌥E.** The fixed version comes back in a small chat at your cursor — click it to copy. On Windows: `Ctrl+Alt+E`.

Quill is built and used on macOS. The Windows build exists and installs, but it isn't tested nearly as much — expect rough edges.

## What it does, and what it doesn't

It fixes mistakes. It does not rewrite you — tone, register and word choices survive. Nothing selected, or nothing wrong with it? Quill does nothing. Your clipboard is borrowed for a split second to read the selection, then put back exactly as it was.

## Never wait on one provider

Add as many models as you like: the top one does the work, the rest wait for the day it doesn't. After enough failures in a row — rate limit, outage, timeout — Quill drops to the next one, then climbs back to the top once the cooldown passes. Both numbers are yours. Reorder with ↑/↓, or point a card at your own endpoint.

## Updates

The pen in the menu bar turns green when a new version is out. Click it, pick the update line — done.

## Privacy

- The selection goes to the model you picked, for correction, and nowhere else.
- Your key lives in a private file on your machine; the history of corrections never leaves it.
- No analytics, no tracking, no other network calls.

## Under the hood

Stack, local build, tests, signing and the release pipeline → [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## License

MIT

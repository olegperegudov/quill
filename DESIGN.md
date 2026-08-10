# Design

The window is a document under correction, not a conversation. Everything below
is recorded from the built world (`src/editor.css`, `src/editor.js`), not from
intentions — when the two disagree, the code is right and this file is stale.

## World

**The dictation desk.** A machine typed the transcript; Quill went over it with a
red ribbon; the finished text lies underneath on the one sheet of paper in the
window. That sheet is the deliverable and the only thing a click copies.

What this rules out, permanently: chat bubbles, avatars, a blue accent, a send
button as the loudest object on screen — the arrangement every LLM utility
ships, and the one Quill wore until 2026-08-10.

Scene that decided dark-on-warm: one window over other apps, many times a day,
often at night. The ground is warm graphite, not blue-black; the paper is the
only bright field, and it is bright on purpose.

## Tokens

Declared on `:root` in `src/editor.css`. Measured contrast against their own
ground in brackets — nothing that carries text sits under 4.5:1.

| Token | Value | Role |
|---|---|---|
| `--ground` | `#17150f` | the desk |
| `--ground-deep` | `#131109` | titlebar, composer |
| `--rule` / `--rule-soft` | `#2e2a20` / `#241f16` | hairlines |
| `--machine` | `#b7ae99` | what the transcriber typed [8.3] |
| `--slug` | `#9a8f78` | block labels [5.7] |
| `--slug-dim` | `#8d8371` | second-rank notes, placeholders [4.9] |
| `--ribbon` | `#d2554a` | struck out — what Quill dropped [4.5] |
| `--ribbon-hi` | `#e5564c` | typed in — what Quill added [5.0] |
| `--paper` | `#efe9dc` | the clean sheet |
| `--paper-ink` | `#16130d` | text on paper [15.3] |
| `--paper-edge` | `#cfc7b4` | the sheet's lower edge |
| `--paper-slug` | `#6f6552` | labels on paper [4.7] |
| `--stamp` / `--stamp-wash` | `#2f6b46` / `#dfe8d9` | the copy flash [5.0] |

Red is the correcting hand, and it is the only chromatic colour on the desk
side. Green appears exactly once — the moment something is copied — and never as
decoration.

## Type

- **Transcript**: `--machine-face` ("Courier New") 12.5px/1.72. Not a costume for
  "technical": this text was typed by a machine, and it is set as such.
- **Clean copy**: `--paper-face` (Georgia) 13.5px/1.62 — a reading
  face for the finished text, with Cyrillic and Latin equally covered.
- **Labels, wordmark, dates, settings rows and every settings value**: the
  `--mono` stack at 0.6–0.74rem, uppercase, tracked `0.1–0.2em`. Silkscreen on a
  machine's case. The stack names Consolas and Segoe UI Mono after the Apple
  faces — falling through to generic `monospace` changed every label's character
  on Windows.
- **Notes and body chrome**: the system sans.

CSP is `default-src 'self'`: no remote fonts. A face this world eventually wants
must be self-hosted as a woff2 inside the app.

## Composition

```
titlebar        feather · QUILL · status                    ⚙ ✕
────────────────────────────────────────────────────────────────
log             (sheets stack down from the titlebar)
  date-sep      mo, aug 10th ─────────────────────────────
  entry         TRANSCRIPT                          5 EDITS
                machine text with ribbon marks in place
                ┌ CLEAN COPY · CLICK TO COPY ────────────┐
                │ the finished text, on paper            │
                └────────────────────────────────────────┘
────────────────────────────────────────────────────────────────
composer        type or paste…                            ↑
```

- One `.entry` per correction; a hairline separates entries, nothing else.
- The stack fills from under the titlebar down, as the approved prototype does;
  dead space belongs at the bottom, where the composer is.
- Nothing changed → no transcript block, one sheet labelled `already clean`.
- The window opens at 520×700, min 360×420, and keeps the size it is dragged to
  (`tauri-plugin-window-state`).

## States

- **Empty**: a blank ruled sheet naming the real hotkey and the composer.
- **Working**: `.ribbon-run` — a ribbon sweeping under the transcript. Held
  still under `prefers-reduced-motion`; it is the page's one authored motion.
- **Copied**: the sheet takes `--stamp-wash`/`--stamp` at once and fades back
  over 0.2s (`.sheet.copied` kills the transition on the way in). 700ms.
- **Error / app note**: `.note`, a ribbon tick in the margin, never a sheet —
  nothing there is yours to copy.
- **Hover**: the sheet gains an edge, not a lift.
- **Focus**: the sheet is a `role="button"` with `tabIndex 0`, Enter and Space
  copy it, and `:focus-visible` rings it in ribbon red. It is the app's primary
  action; mouse-only was a defect.

## Rules that outlive this build

1. The clean sheet is the only copyable surface, and it copies the correction,
   never the markup.
2. Marks stay *in place* in the transcript. A list of edits beside the text was
   considered (the "type specimen" alternate) and refused: the eye should not
   have to travel to learn what changed.
3. Green means copied. Nothing else may borrow it.
4. Icons are drawn (`src/icons.js`) at stroke 1.7 with round caps — no glyph
   from the emoji table stands in for one, and the settings icon is the
   machine's control panel, not a 16px gear turned to mud.
5. New chrome inherits the hairline-and-label grammar: 1px rules, 2px corners on
   paper, 4px on controls, mono small-caps labels. No cards, no glass, no
   gradients.
6. Shared behaviour with Ribbit and Iago stays identical — the copy flash and
   the tray split (left click opens the window, right click the menu) are the
   family's, not Quill's alone.

Prototypes of the two directions that lost, and the chosen one, are kept at
`~/membeme/apps/quill/docs/design-directions.html`.

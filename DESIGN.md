# Design

The window is a shelf you work from the keyboard, not a conversation. Everything
below is recorded from the built world (`src/editor.css`, `src/editor.js`,
`src/keys.js`), not from intentions — when the two disagree, the code is right
and this file is stale.

## World

**The keyboard shelf.** One cold ground, one face, no paper and no second
surface. The finished text is what is on screen; the marks that made it are one
arrow key away, in the same place, not in a second paragraph. Nothing here is
decoration: the loudest thing in the window is which entry the keys are pointing
at, because that is the only state a key press depends on.

What this rules out, permanently: chat bubbles, avatars, a blue accent, a send
button, a second typeface, a bright field of "paper" on a night-time screen, and
any label that repeats under every entry what the bottom strip already says once.

Scene that decided it: a window that drops out from under a menu-bar icon many
times a day, often at night, and is closed again within seconds. It is read at a
glance and driven by two key presses; anything that asks to be aimed at with a
mouse is in the way.

## Ground and light

| Token | Value | What it is |
|---|---|---|
| `--ground` | `#101216` | the window |
| `--raised` / `--raised-hi` | `#171a20` / `#191d24` | the field, boxes in settings |
| `--sel` | `#14171d` | the selected entry |
| `--edge` / `--edge-hi` | `#212630` / `#2c333f` | the field's rim, at rest and holding text |
| `--rule` | `#1c1f26` | hairlines |
| `--ink` / `--ink-2` / `--ink-3` | `#e7e8ea` / `#cdd0d5` / `#9ea3ab` | selected text, unselected, dictated-under-marks |
| `--dim` / `--faint` / `--mute` | `#7c828c` / `#5f656e` / `#565c65` | meta, day labels, the key strip |
| `--accent` | `#e2a13f` | where you are, and what you can press |
| `--del` / `--ins` | `#ef8f84` / `#6fd39a` | what the correction dropped / added |
| `--ok` | `#62c08a` | copied |
| `--find` / `--find-ink` | `#3a2d68` / `#cfc0f5` | a search hit |

Amber is the only accent, and it means one thing: *this is what the keys act on*
— the rail down the selected entry, the "first" tag on the model that runs, the
one link in settings. Green never means an accent; it means done (copied), or
added (a mark). Red only ever means dropped.

The three text tones are a hierarchy, not decoration: the selected entry is
brightest, the rest of the history a step back, and a paragraph showing its marks
steps back further still so the red and green read as marks rather than speckle.

## Type

One face — the system sans — at 13.5px/1.6 for the corrected text. The mono
(`--mono`) is furniture only: day labels, the key strip, settings labels, the
prompt box, endpoints and model ids. Anything the user wrote is in the sans;
anything the app says about it is in the mono, small and upper-case. There is no
third face and no serif: the old world's Georgia "paper" is gone.

## The bar

Quill, field, gear — three boxes of one height (`--bar`, 34px), the two ends
square and equal, the field filling the rest.

- The field is a **pill**: `border-radius: calc(var(--bar) / 2)`, not "rounded
  corners". Its left inset is a full radius, because a round side clips a letter
  set flush against it. The text sits on the centre line by construction
  (7 + 18 + 7 fills the pill exactly), which is what makes the row read as
  symmetric.
- Grown past two lines to hold a pasted paragraph, the pill steps down to a 14px
  radius — round sides start eating the lines.
- The field is the only thing that ever holds focus. That is what makes ⌘V and
  typing work without a click, and it is why the arrows only take the list once
  the field is empty.

## An entry

Time, the edit count, and the text. Nothing else — no "transcript" slug, no
"click to copy" instruction, no repeated arrow legend. The rail on the left says
which one the keys will act on; the strip at the bottom says what the keys do.

The marks are colour plus one stroke: dropped text struck through in `--del`,
added text underlined in `--ins`. No background wash — a wash behind every
inserted comma turns a corrected paragraph into mosaic, and a dictated one
carries two dozen of them.

## The key strip

The bottom edge names what the keys do, and it changes with what the field
holds: the list (`↑↓ history · ←→ was / now · ↵ copy and close · esc close`), a
search (`↵ correct · ↑↓ results · esc clear`), pasted material (`↵ correct ·
⇧↵ new line · esc clear`), and a correction in flight (`↵ take it and close ·
esc stop`). With nothing in the history it shrinks to `esc close`: naming keys
that cannot do anything is a promise the window does not keep.

## Motion

Two pulsing bars where the answer will land, and nothing else. No slide, no
fade-in on the list, no spinner. The bars exist for one reason: the entry's place
is taken the moment Enter is pressed, so the list does not jump under the eye
when the text arrives. `prefers-reduced-motion` stops the pulse.

## What the window is made of

- **The bar** — quill, field, gear.
- **The history** — day labels, entries, newest first, right under the field.
- **The key strip** — one line, bottom edge.
- **Settings / debug log** — replace the history in place; the bar stays, the
  field gives its place to the panel's name, and the gear is the way back.

Empty history is a two-line blank: paste here, or select anywhere and press the
hotkey. It is furniture, not an announcement.

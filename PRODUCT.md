# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

One user, the author of the app: a person who dictates far more text than he
types. Speech goes through a transcriber that returns a wall of words — no
commas, no full stops, no capitals, no paragraph breaks — and that wall is what
he then has to send to a colleague, paste into a ticket, or publish. Quill is
the step between the transcriber and the place the text is going.

The second, older situation: text already sitting in some other app (a message
half-typed, a paragraph in a doc), selected in place, corrected without leaving
that app.

## Product Purpose

Take raw text and give back the same text written properly — spelling,
punctuation, grammar, capitalisation — in Russian or English, without changing
the meaning, the wording, or the tone. It is a corrector, not a rewriter and not
a translator.

Success is that the text can be pasted onward without a second read, and that
the user can see at a glance what was wrong, so the same mistake is his own next
time.

## Positioning

The correction is shown as edits, not as a second paragraph: what was dropped is
struck through, what was added is underlined, in place. A generic chat assistant
answers with a near-identical block of text and leaves the comparison to the
reader — on a dictated page nobody does that comparison.

## Operating Context

- Lives in the macOS menu bar / Windows tray, no Dock icon, running all day.
- Two ways in: the hotkey (⌃⌥E) over a selection anywhere in the OS, and the
  window itself, where text is pasted and sent with Enter.
- **The window is the primary path** (confirmed 2026-08-10). The typical unit of
  work is a dictated paragraph or several, pasted in, corrected, then copied out
  by clicking it and pasted where it was going.
- Typical input length: from one sentence to several paragraphs of transcript.
- History of past corrections is kept and reloaded into the window.
- Model calls go to an ordered stack of providers; if the top one rate-limits or
  fails, the next takes over.

## Capabilities and Constraints

- Tauri 2 + a plain HTML/CSS/JS frontend (no framework, no bundler); one window,
  three views inside it — chat, settings, debug log. Same shape as its siblings
  Ribbit and Iago, and shared plumbing is meant to stay shared.
- Content-Security-Policy is `default-src 'self'`: no remote fonts, scripts,
  images or network calls from the frontend. Anything the design needs must ship
  inside the app.
- The frontend never touches the network or the API key — Rust does.
- The word-level diff is computed in the frontend; past ~1500 changed tokens it
  gives up and renders plain text rather than hanging the window.
- Copying must copy the correction, never the diff markup.
- The selected text is untrusted content; the system prompt forbids the model
  from following instructions found inside it. Not to be weakened.
- Window closes to the tray; the app must survive with zero windows open.
- Verified only through the released build (in-app updater), not local builds.

**Confirmed for the redesign (2026-08-10):** the reply is split in two — the
edits, and the clean final text below it; a click on the clean text copies it and
the whole text flashes, the way Ribbit's log lines do; the window becomes
resizable with a larger default, remembering size and position.

## Brand Commitments

- Name: **Quill**. Mark: the pixel-art feather, one of a set of three — the frog
  (Ribbit, voice to text), the parrot (Iago), the feather (Quill). The set is
  deliberate; the feather is not to be replaced with a generic glyph.
- The apps are siblings: what the frog and the parrot do the same way (tray menu,
  update flow, copy feedback), Quill does the same way too.
- Voice in the UI: lower-case, short, plain English. No exclamation marks, no
  assistant chatter.

## Evidence on Hand

- Working app at `~/quill`, released and self-updating (`olegperegudov/quill`).
- Real screenshots in the README, taken by `_quill_shot.mjs`.
- Real corrections in the local history; real transcriber output as input.
- No customers, no metrics, no testimonials — a personal tool. Nothing of the
  kind is to be invented.

## Product Principles

1. **Show the edits, not just the answer.** The user must see what changed
   without diffing two paragraphs by eye.
2. **The clean text is the deliverable.** Whatever else the window shows, the
   thing that gets copied is unambiguous.
3. **Correct, never rewrite.** Meaning, wording and tone are the user's.
4. **A sibling of the frog and the parrot.** Shared behaviours stay identical
   across the three apps; surprises between them are bugs.
5. **It lives in the tray all day.** Quiet at rest, instant when called, never
   steals focus from the text being corrected.

## Accessibility & Inclusion

Bilingual by requirement (Russian and English, often mixed in one text) — the
type has to carry Cyrillic and Latin equally well. Long dictated texts are read
on screen, so body copy must stay comfortable at paragraph length.

//! Quill's window — the hotkey pops this at the cursor.
//!
//! Flow: select text anywhere + press the hotkey → Rust captures the selection
//! and emits `editor:capture`. The text lands as a transcript, the correction
//! marks it up in place (struck out / typed in), and the finished text settles
//! under it on a paper sheet. Clicking the sheet copies it — that, and only
//! that, is what a click takes: the correction, never the marks. The composer
//! at the bottom sends fresh text through the same path with Enter.

import { initSettings } from "./settings.js";
import { diffWords } from "./diff.js";
import { prettyShortcut } from "./shortcut.js";
import { setIcon } from "./icons.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel) => document.querySelector(sel);
const log = $("#log");
const input = $("#input");
const composer = $("#composer");
const settingsPanel = $("#settings-panel");
const debugPanel = $("#debug-panel");
const settingsBtn = $("#settings-btn");
const sendBtn = $("#send");

// Diagnostics without DevTools (disabled in prod) — goes to the shared debug log.
function dlog(msg) {
  try { invoke("js_debug_log", { msg: String(msg) }); } catch (_) {}
}

const scrollToBottom = () => { log.scrollTop = log.scrollHeight; };

// --- Day separators (copied from Ribbit: weekday + month + ordinal day) ---

function ordinal(n) {
  const s = ["th", "st", "nd", "rd"];
  const v = n % 100;
  return s[(v - 20) % 10] || s[v] || s[0];
}

// e.g. "tu, jun 24th". null/undefined ts → today.
function formatDate(iso) {
  const d = iso ? new Date(iso) : new Date();
  const wd = ["su", "mo", "tu", "we", "th", "fr", "sa"][d.getDay()];
  const mon = d.toLocaleDateString("en-US", { month: "short" }).toLowerCase();
  const day = d.getDate();
  return `${wd}, ${mon} ${day}${ordinal(day)}`;
}

// In chat order (oldest at top), drop a separator above the first message of
// each calendar day. `lastDay` tracks the day at the bottom of the log.
let lastDay = null;
function ensureDay(iso) {
  const label = formatDate(iso);
  if (label === lastDay) return;
  lastDay = label;
  const sep = document.createElement("div");
  sep.className = "date-sep";
  const span = document.createElement("span");
  span.className = "date-sep-label";
  span.textContent = label;
  sep.appendChild(span);
  log.appendChild(sep);
}

// Copy the sheet's text and flash the whole of it — Ribbit's signal, and the one
// the eye is already on, since the cursor is over the text it just took.
async function copySheet(sheet, text) {
  try {
    await invoke("copy_to_clipboard", { text });
    sheet.classList.add("copied");
    setTimeout(() => sheet.classList.remove("copied"), 700);
  } catch (err) {
    dlog(`copy failed: ${err}`);
  }
}

// The label above a block: what it is on the left, the edit count on the right.
function slug(label, count = "") {
  const row = document.createElement("div");
  row.className = "slug";
  const left = document.createElement("span");
  left.textContent = label;
  const right = document.createElement("span");
  right.className = "edits";
  right.textContent = count;
  row.append(left, right);
  return row;
}

const edits = (n) => `${n} ${n === 1 ? "edit" : "edits"}`;

// Paint the transcript with the edits themselves: what Quill dropped is struck
// through, what it typed in is in the ribbon's red. Returns how many edits were
// marked. A text too long to diff (a pasted page) falls back to plain and
// reports none — see MAX_TOKENS in diff.js.
function renderDraft(el, original, corrected) {
  el.textContent = "";
  const ops = diffWords(original, corrected);
  if (!ops) {
    el.textContent = original;
    return 0;
  }
  let marks = 0;
  for (const op of ops) {
    if (op.type === "same") {
      el.append(op.text);
      continue;
    }
    // A replacement is one edit, not two: count the removal and let its
    // insertion ride along.
    if (op.type !== "ins" || !el.lastElementChild || el.lastElementChild.tagName !== "DEL") marks++;
    const mark = document.createElement(op.type === "ins" ? "ins" : "del");
    mark.textContent = op.text;
    el.appendChild(mark);
  }
  return marks;
}

// One correction on the desk: the transcript with its marks, and under it the
// finished text on paper. Built empty (`corrected` null) while the model reads,
// then finished in place by `settle` below — the transcript never jumps.
function addEntry(original) {
  clearEmptyDesk();
  const entry = document.createElement("div");
  entry.className = "entry";
  const head = slug("transcript");
  const draft = document.createElement("div");
  draft.className = "draft";
  draft.textContent = original;
  const run = document.createElement("div");
  run.className = "ribbon-run";
  entry.append(head, draft, run);
  log.appendChild(entry);
  scrollToBottom();
  return { entry, head, draft, run };
}

// The correction came back: mark the transcript, lay the clean sheet under it.
function settle(parts, original, corrected) {
  const { entry, head, draft, run } = parts;
  run.remove();
  const clean = corrected === original;
  if (clean) {
    entry.classList.add("entry--clean");
    head.remove();
    draft.remove();
  } else {
    const marks = renderDraft(draft, original, corrected);
    head.querySelector(".edits").textContent = marks ? edits(marks) : "";
  }

  const sheet = document.createElement("div");
  sheet.className = "sheet";
  // The one thing the app exists to hand over: a control, not a paragraph that
  // happens to listen for clicks.
  sheet.tabIndex = 0;
  sheet.setAttribute("role", "button");
  sheet.setAttribute("aria-label", "Copy the corrected text");
  sheet.append(slug(clean ? "already clean · click to copy" : "clean copy · click to copy"));
  sheet.append(corrected);
  // Copy the correction, never the markup — the struck-through words are the
  // ones the user asked Quill to get rid of.
  sheet.addEventListener("click", () => copySheet(sheet, corrected));
  sheet.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    copySheet(sheet, corrected);
  });
  entry.appendChild(sheet);
  scrollToBottom();
}

// Nothing on the desk yet: a blank sheet with the two ways in typed on it.
// Rendered once at boot and cleared by the first thing that lands in the log —
// an empty window that says nothing reads as a broken one.
async function showEmptyDesk() {
  if (log.querySelector(".entry, .note")) return;
  let keys = "⌃⌥E";
  try {
    const s = await invoke("get_shortcut");
    if (s) keys = prettyShortcut(s, navigator.platform.toLowerCase().includes("mac"));
  } catch (_) {}
  const blank = document.createElement("div");
  blank.className = "blank";
  blank.innerHTML =
    `<div class="blank-sheet"><p>Paste text below — Enter corrects it.</p>` +
    `<p>Or select it anywhere and press <kbd>${keys}</kbd>.</p></div>`;
  log.appendChild(blank);
}

const clearEmptyDesk = () => log.querySelector(".blank")?.remove();

// A note from the app itself — no key yet, no Accessibility, a failed call.
// Nothing here is copyable, so it is not a sheet.
function addNote(text, { error = false } = {}) {
  clearEmptyDesk();
  const note = document.createElement("div");
  note.className = error ? "note note--error" : "note";
  note.textContent = text;
  log.appendChild(note);
  scrollToBottom();
  return note;
}

// Corrections in flight: id → its unfinished entry. While any are running, the
// send button turns into a stop button. The correction is a single
// non-streaming request, so "stop" means: drop the unfinished entry and ignore
// whatever the call eventually returns, freeing the user to edit and resend.
const inFlight = new Map();
let correctionId = 0;

function reflectGenerating() {
  const busy = inFlight.size > 0;
  composer.classList.toggle("generating", busy);
  sendBtn.title = busy ? "Stop" : "Send (Enter)";
  sendBtn.setAttribute("aria-label", busy ? "Stop" : "Send");
}

function stopGenerating() {
  for (const parts of inFlight.values()) parts.entry.remove();
  inFlight.clear();
  reflectGenerating();
  input.focus();
}

// Send `text` through correct→settle. Each call owns its own entry, so
// concurrent corrections resolve into their own slots.
async function runCorrection(text) {
  const id = ++correctionId;
  ensureDay();
  const parts = addEntry(text);
  inFlight.set(id, parts);
  reflectGenerating();
  try {
    const corrected = await invoke("editor_correct", { text });
    if (!inFlight.has(id)) return; // stopped while we were waiting → discard
    settle(parts, text, corrected);
  } catch (err) {
    if (!inFlight.has(id)) return; // stopped while we were waiting → discard
    parts.entry.remove();
    addNote(String(err), { error: true });
  } finally {
    inFlight.delete(id);
    reflectGenerating();
  }
}

async function loadHistory() {
  try {
    const entries = await invoke("get_log_history", { limit: 50 });
    if (!entries || entries.length === 0) return;
    // History comes newest-first; the desk reads oldest-at-top.
    clearEmptyDesk();
    for (const e of entries.slice().reverse()) {
      const orig = e.original || "";
      const corr = e.corrected || "";
      ensureDay(e.ts);
      settle(addEntry(orig), orig, corr);
    }
    scrollToBottom();
  } catch (err) {
    dlog(`loadHistory failed: ${err}`);
  }
}

// --- Composer ---

function autoGrow() {
  input.style.height = "auto";
  input.style.height = Math.min(input.scrollHeight, 140) + "px";
}

function send() {
  const text = input.value.trim();
  if (!text) return;
  input.value = "";
  autoGrow();
  runCorrection(text);
}

$("#composer").addEventListener("submit", (e) => {
  e.preventDefault();
  // The same button is "send" at rest and "stop" while a correction runs.
  if (inFlight.size > 0) stopGenerating();
  else send();
});
input.addEventListener("input", autoGrow);
input.addEventListener("keydown", (e) => {
  // Enter sends; Shift+Enter is a newline.
  if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); }
});

// The chrome's glyphs are drawn, not typed: one stroke weight across the gear,
// the cross, the prompt and the arrows in the model stack (icons.js).
setIcon(settingsBtn, "gear");
setIcon($("#close"), "close");
setIcon($("#debug-btn"), "prompt");
setIcon($("#debug-close"), "close");
setIcon($(".i-send"), "send", 16);

// --- Views (Ribbit-style: one window, the titlebar stays, the body swaps) ---

// "chat" | "settings" | "debug". The titlebar — and so the gear — is always
// visible, so the gear toggles chat ↔ settings from either side.
let currentView = "chat";
function setView(name) {
  currentView = name;
  const chat = name === "chat";
  log.style.display = chat ? "" : "none";
  composer.style.display = chat ? "" : "none";
  settingsPanel.style.display = name === "settings" ? "flex" : "none";
  debugPanel.style.display = name === "debug" ? "flex" : "none";
  settingsBtn.classList.toggle("active", !chat); // gear shows it's "in settings"
  if (chat) { scrollToBottom(); input.focus(); }
}

// The gear flips between the chat and settings (from debug it returns to chat).
settingsBtn.addEventListener("click", () => setView(currentView === "chat" ? "settings" : "chat"));
$("#close").addEventListener("click", () => invoke("close_editor"));

// Debug log is reached from settings and steps back to it.
$("#debug-btn").addEventListener("click", async () => {
  $("#debug-content").textContent = await invoke("get_debug_log");
  setView("debug");
});
$("#debug-close").addEventListener("click", () => setView("settings"));

// Esc peels back one layer: debug → settings → chat → hide the window. While a
// shortcut capture is live, settings.js owns Esc (cancels it), so we defer via
// the `.capturing` class it sets on the kbd.
window.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if ($("#shortcut-display")?.classList.contains("capturing")) return;
  e.preventDefault();
  if (inFlight.size > 0) return stopGenerating(); // Esc stops generation first
  if (currentView === "debug") return setView("settings");
  if (currentView === "settings") return setView("chat");
  invoke("close_editor");
});

// --- Events from Rust ---

// Hotkey captured a selection (may be empty if nothing was selected / capture
// failed): show it and correct it, or just focus the composer to type.
listen("editor:capture", (e) => {
  const text = (e.payload || "").trim();
  if (text) runCorrection(text);
  else { input.focus(); scrollToBottom(); }
});

// Hotkey fired without Accessibility — macOS already showed its dialog; we leave
// one quiet inline note instead of a blocking overlay.
listen("editor:need-access", () => {
  ensureDay();
  addNote(
    "I need Accessibility access to read the selected text. " +
      "macOS already asked — enable Quill and press ⌃⌥E again (settings are behind ⚙)."
  );
});

// An update waiting is announced on the menu-bar icon (green pen + an install
// line in its menu). The window says nothing about it.

// Bring up history + wire settings. On first run (no API key yet) land on the
// settings view so the window the tray/hotkey reveals isn't a dead end — Rust
// shows this window on a keyless launch.
async function boot() {
  await loadHistory();
  showEmptyDesk();
  try {
    const cfg = await initSettings();
    if (cfg && !cfg.has_api_key) setView("settings");
  } catch (err) {
    dlog(`initSettings failed: ${err}`);
  }
}

boot();
dlog("chat window ready");

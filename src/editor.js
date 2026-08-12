//! Quill's window — the tray icon or the hotkey pops this under the pen.
//!
//! One field at the top does two jobs: while it holds a word it searches the
//! history, and Enter sends whatever is in it to be corrected. Everything else
//! is keys. The field is always focused, so ⌘V lands in it without a click; the
//! arrows only take over once the field is empty, because a pasted paragraph
//! has to stay editable.
//!
//! Two presses cover the whole job: Enter corrects, Enter copies the result and
//! closes the window. The second one may be given in advance — press it while
//! the model is still reading and the window goes away now, the correction
//! reaching the clipboard on its own a couple of seconds later.

import { initSettings, refitPrompt } from "./settings.js";
import { diffWords } from "./diff.js";
import { keyAction } from "./keys.js";
import { prettyShortcut } from "./shortcut.js";
import { setIcon } from "./icons.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel) => document.querySelector(sel);
const log = $("#log");
const input = $("#input");
const fieldWrap = $("#field-wrap");
const keyStrip = $("#keys");
const settingsPanel = $("#settings-panel");
const debugPanel = $("#debug-panel");
const settingsBtn = $("#settings-btn");
const panelTitle = $("#panel-title");

// Diagnostics without DevTools (disabled in prod) — goes to the shared debug log.
function dlog(msg) {
  try { invoke("js_debug_log", { msg: String(msg) }); } catch (_) {}
}

const IS_MAC = navigator.userAgent.includes("Mac");

// --- State -------------------------------------------------------------
//
// `entries` is the history, newest first — the order it is read in, so an index
// into it is what the arrows move and what a click sets. An unfinished
// correction is an entry too (`state: "working"`), which is why its place in
// the list is already taken when the answer arrives.

let entries = [];
let uid = 0;
let sel = 0;
let mode = "list";        // "list" | "search" | "material" — set by what the field holds
let copiedId = null;      // the entry whose meta says "copied"
const showWas = new Set(); // entries currently showing the dictated text instead
let hotkey = "⌃⌥E";

const working = () => entries.some((e) => e.state === "working");

// --- Small formatting --------------------------------------------------

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

const pad = (n) => String(n).padStart(2, "0");
function formatTime(iso) {
  const d = iso ? new Date(iso) : new Date();
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

// The ops and the edit count are the same walk, and the list re-renders on every
// keystroke — so each entry works them out once and keeps them.
function marksOf(e) {
  if (e.marks) return e.marks;
  const ops = e.corrected === null ? null : diffWords(e.original, e.corrected);
  let count = 0;
  if (ops) {
    ops.forEach((op, i) => {
      if (op.type === "same") return;
      // A replacement is one edit, not two: count the removal and let its
      // insertion ride along.
      if (op.type === "ins" && ops[i - 1]?.type === "del") return;
      count++;
    });
  }
  e.marks = { ops, count };
  return e.marks;
}

const editsLabel = (n) => (n === 1 ? "1 edit" : `${n} edits`);

// --- Rendering ---------------------------------------------------------

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// The dictated text with the correction's marks in it: what was dropped struck
// through, what was added underlined.
function paintMarks(node, ops, original) {
  if (!ops) { node.textContent = original; return; }
  for (const op of ops) {
    if (op.type === "same") { node.append(op.text); continue; }
    const mark = el(op.type === "ins" ? "ins" : "del", null, op.text);
    node.appendChild(mark);
  }
}

function metaRow(e) {
  const row = el("div", "meta");
  row.appendChild(el("span", null, formatTime(e.ts)));
  const add = (className, text) => {
    row.appendChild(el("span", "sep", "·"));
    row.appendChild(el("span", className, text));
  };
  // The reason, not just the fact: "failed" alone sends you to the debug log to
  // find out that a provider ran out of credit.
  if (e.state === "failed") add("failed", e.error || "failed");
  else {
    const { count } = marksOf(e);
    add(null, count ? editsLabel(count) : "no edits");
    if (showWas.has(e.id) && count) add(null, "was");
  }
  if (copiedId === e.id) add("copied", "copied");
  return row;
}

function entryNode(e, index) {
  const node = el("div", "entry");
  node.dataset.i = String(index);
  if (index === sel) node.classList.add("sel");

  if (e.state === "working") {
    node.append(el("div", "bar-a"), el("div", "bar-b"));
    const meta = el("div", "meta");
    meta.appendChild(el("span", null, formatTime(e.ts)));
    meta.appendChild(el("span", "sep", "·"));
    meta.appendChild(el("span", null, e.pending ? "correcting… will be copied" : "correcting…"));
    node.appendChild(meta);
    return node;
  }

  const text = el("div", "txt");
  const { ops, count } = marksOf(e);
  const was = showWas.has(e.id) && count > 0;
  if (was) { node.classList.add("was"); paintMarks(text, ops, e.original); }
  else text.textContent = e.state === "failed" ? e.original : e.corrected;
  node.append(text, metaRow(e));
  return node;
}

// Searched over both halves: you remember what you dictated as often as what
// you sent.
const hay = (e) => [e.corrected || "", e.original || ""];

// Which entries the keys and the list are working on: everything, or just what
// the field found.
function visible() {
  if (mode !== "search") return entries.map((_, i) => i);
  const q = input.value.trim().toLowerCase();
  return entries
    .map((_, i) => i)
    .filter((i) => hay(entries[i]).some((s) => s.toLowerCase().includes(q)));
}

function renderList() {
  let day = null;
  entries.forEach((e, i) => {
    const label = formatDate(e.ts);
    if (label !== day) { day = label; log.appendChild(el("div", "day", label)); }
    log.appendChild(entryNode(e, i));
  });
}

function renderSearch() {
  const q = input.value.trim();
  const hits = visible();
  if (!hits.length) {
    const blank = el("div", "blank");
    blank.append("Nothing found. Press Enter and I'll correct what you typed.");
    log.appendChild(blank);
    return;
  }
  log.appendChild(el("div", "day", hits.length === 1 ? "1 result" : `${hits.length} results`));
  for (const i of hits) {
    const e = entries[i];
    const from = hay(e).find((s) => s.toLowerCase().includes(q.toLowerCase())) || "";
    const at = from.toLowerCase().indexOf(q.toLowerCase());
    const node = el("div", "entry");
    node.dataset.i = String(i);
    if (i === sel) node.classList.add("sel");
    const snip = el("div", "snip");
    const inner = el("div", "snip-inner");
    inner.append(from.slice(0, at));
    inner.appendChild(el("mark", null, from.slice(at, at + q.length)));
    inner.append(from.slice(at + q.length));
    snip.appendChild(inner);
    node.append(snip, metaRow(e));
    log.appendChild(node);
  }
}

// The hit's own line in the middle, one line above it and one below. Done after
// the text is laid out, because that is the only moment its lines exist.
function clampSnippets() {
  for (const snip of log.querySelectorAll(".snip")) {
    const inner = snip.firstElementChild;
    const mark = inner.querySelector("mark");
    if (!mark) continue;
    const LINE = parseFloat(getComputedStyle(inner).lineHeight);
    const total = Math.round(inner.scrollHeight / LINE);
    if (total <= 3) continue;
    const line = Math.round((mark.offsetTop - inner.offsetTop) / LINE);
    const off = Math.min(Math.max(line - 1, 0), total - 3);
    inner.style.marginTop = `${-off * LINE}px`;
    if (off > 0) snip.classList.add("clip-top");
    if (off + 3 < total) snip.classList.add("clip-bot");
  }
}

// Nothing on the shelf yet: the two ways in, and nothing else.
function renderBlank() {
  const blank = el("div", "blank");
  blank.append("Paste text here — Enter corrects it.");
  blank.appendChild(el("br"));
  blank.append("Or select it anywhere and press ");
  blank.appendChild(el("kbd", null, hotkey));
  blank.append(".");
  log.appendChild(blank);
}

const KEY_HINTS = {
  list: [["↑↓", "history"], ["←→", "was / now"], ["↵", "copy and close"], ["esc", "close"]],
  search: [["↵", "correct"], ["↑↓", "results"], ["esc", "clear"]],
  material: [["↵", "correct"], ["⇧↵", "new line"], ["esc", "clear"]],
  // While the model reads, Enter is a promise: take whatever comes back.
  working: [["↵", "take it and close"], ["esc", "stop"]],
};

function renderKeys() {
  keyStrip.textContent = "";
  const hints = working() ? KEY_HINTS.working
    : mode === "list" && !entries.length ? [["esc", "close"]]
      : KEY_HINTS[mode];
  for (const [key, what] of hints) {
    const span = el("span");
    span.appendChild(el("b", null, key));
    span.append(what);
    keyStrip.appendChild(span);
  }
}

// The field's shape is also the mode: a pill searches, a grown box holds
// material. A pasted paragraph is not a query, and must not wipe the history
// with "nothing found".
function syncField() {
  autoGrow();
  const tall = input.scrollHeight > parseFloat(getComputedStyle(input).lineHeight) * 2;
  fieldWrap.classList.toggle("hot", input.value.length > 0);
  fieldWrap.classList.toggle("tall", tall);
  mode = !input.value.trim() ? "list" : tall ? "material" : "search";
}

function render({ focus = false } = {}) {
  syncField();
  log.textContent = "";
  if (mode === "search") renderSearch();
  else if (entries.length) renderList();
  else renderBlank();
  clampSnippets();
  renderKeys();

  const current = log.querySelector(".entry.sel");
  if (current) current.scrollIntoView({ block: "nearest" });
  if (focus) focusField();
}

function focusField() {
  input.focus();
  input.selectionStart = input.selectionEnd = input.value.length;
}

function autoGrow() {
  input.style.height = "auto";
  input.style.height = `${input.scrollHeight}px`;
}

// --- Doing things ------------------------------------------------------

function move(step) {
  const list = visible();
  if (!list.length) return;
  const at = Math.max(0, list.indexOf(sel));
  sel = list[Math.min(list.length - 1, Math.max(0, at + step))];
}

// Copying is the window's whole point, so it also ends the window's job: the
// text is in the clipboard and there is nothing left to look at.
async function copySelected({ close = true } = {}) {
  const e = entries[sel];
  if (!e || e.state !== "done") return;
  await copyEntry(e);
  if (close) invoke("close_editor");
}

async function copyEntry(e) {
  try {
    await invoke("copy_to_clipboard", { text: e.corrected });
    copiedId = e.id;
    render();
  } catch (err) {
    dlog(`copy failed: ${err}`);
  }
}

// Send whatever is in the field. Its answer's place in the list is taken now,
// selected, so the list doesn't move under the eye when the text lands.
async function send() {
  const text = input.value.trim();
  input.value = "";
  if (!text) return render({ focus: true });
  const e = { id: ++uid, ts: new Date().toISOString(), original: text, corrected: null, state: "working" };
  entries.unshift(e);
  sel = 0;
  render({ focus: true });

  try {
    const corrected = await invoke("editor_correct", { text });
    if (e.state === "stopped") return;
    e.corrected = corrected;
    e.state = "done";
    // The promise given in advance: the window is already gone, the clipboard
    // is the only thing left to answer.
    if (e.pending) { e.pending = false; await copyEntry(e); }
    else render();
  } catch (err) {
    if (e.state === "stopped") return;
    e.state = "failed";
    e.error = String(err);
    dlog(`correction failed: ${err}`);
    render();
  }
}

// The correction is one non-streaming request, so stopping means: drop the
// unfinished entry and ignore whatever comes back, freeing the text to be
// edited and sent again.
function stopWorking() {
  const stopped = entries.filter((e) => e.state === "working");
  for (const e of stopped) e.state = "stopped";
  entries = entries.filter((e) => e.state !== "stopped");
  sel = 0;
  render({ focus: true });
}

// --- Views (one window, the bar stays, the body swaps) -----------------

let currentView = "chat";
function setView(name) {
  currentView = name;
  const chat = name === "chat";
  log.style.display = chat ? "" : "none";
  fieldWrap.style.display = chat ? "" : "none";
  panelTitle.style.display = chat ? "none" : "flex";
  panelTitle.textContent = name === "settings" ? "Settings" : "Debug log";
  settingsPanel.style.display = name === "settings" ? "flex" : "none";
  debugPanel.style.display = name === "debug" ? "flex" : "none";
  settingsBtn.classList.toggle("active", !chat);
  keyStrip.textContent = "";
  if (!chat) {
    const span = el("span");
    span.appendChild(el("b", null, "esc"));
    span.append(name === "debug" ? "back to settings" : "back");
    keyStrip.appendChild(span);
  }
  // The prompt box sizes itself to its text, and a hidden panel has no height to
  // measure — so it is measured here, the moment settings become visible.
  if (name === "settings") refitPrompt();
  if (chat) render({ focus: true });
}

// --- Keys --------------------------------------------------------------

// Flip the marks on the selected entry — left shows what was dictated, right
// the finished text.
function setWas(on) {
  const current = entries[sel];
  if (!current || current.state !== "done") return;
  if (on) showWas.add(current.id);
  else showWas.delete(current.id);
  render();
}

// Agreed to in advance: the window goes now, the clipboard is filled when the
// answer arrives.
function takeInAdvance() {
  entries.find((e) => e.state === "working").pending = true;
  render();
  invoke("close_editor");
}

const DO = {
  back: () => setView(currentView === "debug" ? "settings" : "chat"),
  send,
  "copy-close": () => copySelected(),
  "pending-close": takeInAdvance,
  stop: stopWorking,
  clear: () => { input.value = ""; render({ focus: true }); },
  close: () => invoke("close_editor"),
  prev: () => { move(-1); render(); },
  next: () => { move(1); render(); },
  was: () => setWas(true),
  now: () => setWas(false),
};

function onKeyDown(e) {
  const action = keyAction(e, {
    view: currentView,
    hasText: input.value.length > 0,
    working: working(),
    capturing: $("#shortcut-display")?.classList.contains("capturing"),
  });
  if (!action) return;
  e.preventDefault();
  DO[action]();
}

// --- Wiring ------------------------------------------------------------

setIcon(settingsBtn, "gear", 22);
setIcon($("#debug-btn"), "prompt");

input.addEventListener("input", () => {
  // A fresh query starts on the first thing it found — so the mode has to be
  // settled before asking what is visible.
  syncField();
  const list = visible();
  if (list.length && !list.includes(sel)) sel = list[0];
  render();
});

window.addEventListener("keydown", onKeyDown);

// A click does what Enter does minus the closing: the mouse is used while
// looking at the list, and taking the window away mid-look is rude.
log.addEventListener("click", (e) => {
  const row = e.target.closest("[data-i]");
  if (!row) return;
  sel = Number(row.dataset.i);
  copySelected({ close: false });
});

settingsBtn.addEventListener("click", () => setView(currentView === "chat" ? "settings" : "chat"));
$("#debug-btn").addEventListener("click", async () => {
  $("#debug-content").textContent = await invoke("get_debug_log");
  setView("debug");
});

// The window is shown by the tray and the hotkey, and it is typed into
// immediately — so the field takes focus back every time the window comes up.
window.addEventListener("focus", () => { if (currentView === "chat") focusField(); });

// --- Events from Rust --------------------------------------------------

// The hotkey captured a selection: it lands in the field, ready to be corrected
// with Enter — or edited first, which is the reason it isn't sent on its own.
listen("editor:capture", (e) => {
  const text = (e.payload || "").trim();
  setView("chat");
  if (text) input.value = text;
  copiedId = null;
  render({ focus: true });
});

// Hotkey fired without Accessibility — macOS already showed its dialog; we leave
// one quiet note instead of a blocking overlay.
listen("editor:need-access", () => {
  const note = el("div", "note note--error",
    "I need Accessibility access to read the selected text. macOS already asked — " +
    `enable Quill and press ${hotkey} again (settings are behind the gear).`);
  log.prepend(note);
});

// An update waiting is announced on the menu-bar icon (green pen + an install
// line in its menu). The window says nothing about it.

async function loadHistory() {
  try {
    const rows = await invoke("get_log_history", { limit: 50 });
    if (!rows) return;
    // History arrives newest-first, which is the order it is read in.
    entries = rows.map((r) => ({
      id: ++uid,
      ts: r.ts,
      original: r.original || "",
      corrected: r.corrected || "",
      state: "done",
    }));
  } catch (err) {
    dlog(`loadHistory failed: ${err}`);
  }
}

async function boot() {
  try {
    const s = await invoke("get_shortcut");
    if (s) hotkey = prettyShortcut(s, IS_MAC);
  } catch (_) {}
  await loadHistory();
  render({ focus: true });
  try {
    // First run (no key yet): land on settings, so the window the tray reveals
    // isn't a dead end.
    const cfg = await initSettings();
    if (cfg && !cfg.has_api_key) setView("settings");
  } catch (err) {
    dlog(`initSettings failed: ${err}`);
  }
}

boot();
dlog("quill window ready");

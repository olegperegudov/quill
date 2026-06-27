//! Quill chat window — the hotkey pops this at the cursor.
//!
//! Flow: select text anywhere + press the hotkey → Rust captures the selection
//! and emits `editor:capture`. We show it as your bubble, run the correction,
//! and drop the result in as a reply bubble. Click any bubble to copy it to the
//! clipboard (then paste it yourself). The composer at the bottom lets you type
//! or paste a fresh message — Enter sends it through the same correct→reply path,
//! so re-polishing is just: click your bubble (copies), paste, tweak, Enter.

import { initSettings } from "./settings.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel) => document.querySelector(sel);
const log = $("#log");
const input = $("#input");
const composer = $("#composer");
const settingsPanel = $("#settings-panel");
const debugPanel = $("#debug-panel");
const settingsBtn = $("#settings-btn");

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

// Copy a bubble's text and flash a brief "скопировано" on it.
async function copyBubble(bubble, text) {
  try {
    await invoke("copy_to_clipboard", { text });
    bubble.classList.add("copied");
    setTimeout(() => bubble.classList.remove("copied"), 900);
  } catch (err) {
    dlog(`copy failed: ${err}`);
  }
}

// One chat row. role: "user" (your text), "bot" (the correction), "system"
// (a note — not copyable). `clean` marks a correction that changed nothing.
function addMessage(role, text, { clean = false } = {}) {
  const msg = document.createElement("div");
  msg.className = `msg msg-${role}`;

  const bubble = document.createElement("div");
  bubble.className = "bubble";
  bubble.textContent = text;

  if (role !== "system") {
    bubble.title = "нажми, чтобы скопировать";
    bubble.addEventListener("click", () => copyBubble(bubble, text));
    if (clean) {
      bubble.classList.add("bubble--clean");
      const tag = document.createElement("span");
      tag.className = "clean-tag";
      tag.textContent = "уже чисто";
      msg.appendChild(bubble);
      msg.appendChild(tag);
      log.appendChild(msg);
      scrollToBottom();
      return msg;
    }
  }

  msg.appendChild(bubble);
  log.appendChild(msg);
  scrollToBottom();
  return msg;
}

// A reply bubble with animated dots while the LLM is thinking.
function addPending() {
  const msg = document.createElement("div");
  msg.className = "msg msg-bot";
  msg.innerHTML = `<div class="bubble bubble--pending"><span></span><span></span><span></span></div>`;
  log.appendChild(msg);
  scrollToBottom();
  return msg;
}

// Send `text` through correct→reply. Each call owns its own pending bubble, so
// concurrent corrections resolve into their own slots.
async function runCorrection(text) {
  ensureDay();
  addMessage("user", text);
  const pending = addPending();
  try {
    const corrected = await invoke("editor_correct", { text });
    pending.remove();
    addMessage("bot", corrected, { clean: corrected === text });
  } catch (err) {
    pending.remove();
    addMessage("system", `⚠ ${err}`);
  }
}

async function loadHistory() {
  try {
    const entries = await invoke("get_log_history", { limit: 50 });
    if (!entries || entries.length === 0) return;
    // History comes newest-first; a chat reads oldest-at-top.
    for (const e of entries.slice().reverse()) {
      const orig = e.original || "";
      const corr = e.corrected || "";
      ensureDay(e.ts);
      addMessage("user", orig);
      addMessage("bot", corr, { clean: orig === corr });
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

$("#composer").addEventListener("submit", (e) => { e.preventDefault(); send(); });
input.addEventListener("input", autoGrow);
input.addEventListener("keydown", (e) => {
  // Enter sends; Shift+Enter is a newline.
  if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); }
});

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
  addMessage(
    "system",
    "Нужен доступ в «Универсальный доступ», чтобы я видел выделенный текст. " +
      "macOS уже спросил — включи Quill и нажми ⌃⌥E снова (настроить можно через ⚙)."
  );
});

// An update is waiting — light the gear so it's noticeable; the install button
// itself lives in settings.
listen("update-available", () => {
  const gear = $("#settings-btn");
  gear.classList.add("update-available");
  gear.title = "Доступно обновление — открой настройки";
});

// Bring up history + wire settings. On first run (no API key yet) land on the
// settings view so the window the tray/hotkey reveals isn't a dead end — Rust
// shows this window on a keyless launch.
async function boot() {
  loadHistory();
  try {
    const cfg = await initSettings();
    if (cfg && !cfg.has_api_key) setView("settings");
  } catch (err) {
    dlog(`initSettings failed: ${err}`);
  }
}

boot();
dlog("chat window ready");

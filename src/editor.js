//! Quill chat window — the hotkey pops this at the cursor.
//!
//! Flow: select text anywhere + press the hotkey → Rust captures the selection
//! and emits `editor:capture`. We show it as your bubble, run the correction,
//! and drop the result in as a reply bubble. Click any bubble to copy it to the
//! clipboard (then paste it yourself). The composer at the bottom lets you type
//! or paste a fresh message — Enter sends it through the same correct→reply path,
//! so re-polishing is just: click your bubble (copies), paste, tweak, Enter.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel) => document.querySelector(sel);
const log = $("#log");
const input = $("#input");

// Diagnostics without DevTools (disabled in prod) — goes to the shared debug log.
function dlog(msg) {
  try { invoke("js_debug_log", { msg: String(msg) }); } catch (_) {}
}

const scrollToBottom = () => { log.scrollTop = log.scrollHeight; };

function hideEmpty() {
  const e = $("#empty");
  if (e) e.style.display = "none";
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
  hideEmpty();
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
    hideEmpty();
    // History comes newest-first; a chat reads oldest-at-top.
    for (const e of entries.slice().reverse()) {
      const orig = e.original || "";
      const corr = e.corrected || "";
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

// --- Window controls ---

$("#settings-btn").addEventListener("click", () => invoke("show_main_window"));
$("#close").addEventListener("click", () => invoke("close_editor"));
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") { e.preventDefault(); invoke("close_editor"); }
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
  hideEmpty();
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

loadHistory();
dlog("chat window ready");

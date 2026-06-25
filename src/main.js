//! Quill main window.
//!
//! The window is secondary — the product is the global hotkey, which the Rust
//! side owns. The body is a chat-style log of what Quill corrected (newest on
//! top); each row shows the polished result and a clock you press-and-hold to
//! peek at the original. Settings (model, key, hotkey) hide behind the gear so
//! the log greets you clean.

import { shortcutFromEvent, prettyShortcut } from "./shortcut.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;

const $ = (sel) => document.querySelector(sel);

// Show the hotkey the way each OS writes it (⌃⌥E on Mac, "Ctrl + Alt + E" on
// Windows) while the stored binding stays Tauri's lowercase form.
const IS_MAC = navigator.userAgent.includes("Mac");
const pretty = (raw) => prettyShortcut(raw, IS_MAC);

// A small clock — its hands are the "rewind to the original" affordance.
const CLOCK_SVG = `<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="6"/><path d="M8 4.7V8l2.1 1.6"/></svg>`;

// Diagnostics without DevTools (disabled in prod builds): goes to the debug log.
function dlog(msg) {
  try { invoke("js_debug_log", { msg: String(msg) }); } catch (_) {}
}

function formatTime(iso) {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
}

// --- Live status (header subtitle) ---

let statusResetTimer = null;
function setStatus(text, kind = "idle") {
  const el = $("#status-detail");
  el.className = `status-detail ${kind}`;
  el.textContent = text;
  clearTimeout(statusResetTimer);
  // Settle back to "Ready" after a terminal state so the line doesn't lie.
  if (kind === "done" || kind === "error") {
    statusResetTimer = setTimeout(() => setStatus("Ready", "idle"), 4000);
  }
}

// --- View switching (log <-> settings) ---

let currentView = "log";
function showView(name) {
  currentView = name;
  $("#log-view").style.display = name === "log" ? "flex" : "none";
  $("#settings-panel").style.display = name === "settings" ? "flex" : "none";
  $("#settings-btn").classList.toggle("active", name === "settings");
}

// --- Log ---

function renderHistory(entries) {
  const list = $("#history-list");
  list.innerHTML = "";
  if (!entries || entries.length === 0) {
    $("#history-empty").style.display = "flex";
    return;
  }
  $("#history-empty").style.display = "none";
  for (const e of entries) {
    list.appendChild(logRow(e));
  }
}

// One correction as a chat-style row: time, the polished text, and — when the
// text actually changed — a clock you hold down to reveal the original.
function logRow(e) {
  const row = document.createElement("div");
  row.className = "log-entry";

  const time = document.createElement("span");
  time.className = "log-time";
  time.textContent = e.ts ? formatTime(e.ts) : "";

  const text = document.createElement("span");
  text.className = "log-text";
  text.textContent = e.corrected || "";

  row.append(time, text);

  const changed = e.original !== e.corrected;
  if (changed) {
    const clock = document.createElement("button");
    clock.className = "log-clock";
    clock.title = "hold to see the original";
    clock.tabIndex = -1;
    clock.innerHTML = CLOCK_SVG;

    const showOriginal = (ev) => {
      ev.preventDefault();
      // Capture the pointer so release restores even if the cursor drifts off.
      try { clock.setPointerCapture(ev.pointerId); } catch (_) {}
      text.textContent = e.original;
      text.classList.add("is-original");
      row.classList.add("peeking");
    };
    const restore = () => {
      text.textContent = e.corrected;
      text.classList.remove("is-original");
      row.classList.remove("peeking");
    };
    clock.addEventListener("pointerdown", showOriginal);
    clock.addEventListener("pointerup", restore);
    clock.addEventListener("pointercancel", restore);
    row.appendChild(clock);
  } else {
    const clean = document.createElement("span");
    clean.className = "log-clean";
    clean.textContent = "already clean";
    row.appendChild(clean);
  }
  return row;
}

async function loadHistory() {
  try {
    const entries = await invoke("get_log_history", { limit: 100 });
    renderHistory(entries);
  } catch (err) {
    dlog(`loadHistory failed: ${err}`);
  }
}

// Prepend a fresh correction the moment it happens, without a full reload.
function prependCorrection(e) {
  $("#history-empty").style.display = "none";
  const list = $("#history-list");
  list.insertBefore(logRow({ ...e, ts: new Date().toISOString() }), list.firstChild);
}

// --- Provider + key ---

let providers = [];

async function loadProviders(currentProvider, providerKeys) {
  providers = await invoke("list_llm_providers");
  const sel = $("#provider-select");
  sel.innerHTML = "";
  for (const p of providers) {
    const opt = document.createElement("option");
    opt.value = p.name;
    opt.textContent = p.label;
    if (p.name === currentProvider) opt.selected = true;
    sel.appendChild(opt);
  }
  reflectKeyState(currentProvider, providerKeys);
}

function reflectKeyState(provider, providerKeys) {
  const has = providerKeys && providerKeys[provider];
  $("#key-saved").style.display = has ? "inline-flex" : "none";
  $("#key-input").style.display = has ? "none" : "block";
  $("#key-input").value = "";
}

async function refreshConfig() {
  const cfg = await invoke("get_config");
  await loadProviders(cfg.llm_provider, cfg.llm_provider_keys);
  return cfg;
}

// --- Shortcut capture ---

let capturing = false;

function setupShortcutCapture() {
  const disp = $("#shortcut-display");
  disp.addEventListener("click", () => {
    if (capturing) return;
    capturing = true;
    disp.classList.add("capturing");
    disp.textContent = "press keys…";
  });

  window.addEventListener("keydown", async (e) => {
    if (!capturing) return;
    e.preventDefault();
    if (e.key === "Escape") {
      capturing = false;
      disp.classList.remove("capturing");
      disp.textContent = pretty(await invoke("get_shortcut"));
      return;
    }
    const { parts, complete } = shortcutFromEvent(e);
    disp.textContent = parts.length ? pretty(parts.join("+")) : "press keys…";
    if (!complete) return;

    const shortcut = parts.join("+");
    capturing = false;
    disp.classList.remove("capturing");
    try {
      await invoke("set_shortcut", { shortcut });
      disp.textContent = pretty(shortcut);
      setStatus(`Hotkey set to ${pretty(shortcut)}`, "done");
    } catch (err) {
      disp.textContent = pretty(await invoke("get_shortcut"));
      setStatus(`Couldn't set hotkey: ${err}`, "error");
    }
  });
}

// --- Update flow ---

async function setupUpdates() {
  $("#version").textContent = "v" + (await invoke("get_current_version"));

  const btn = $("#update-btn");
  let pendingVersion = null; // set once an update is known to be available

  function markAvailable(version) {
    pendingVersion = version;
    btn.textContent = `install v${version}`;
    btn.classList.add("ready");
    btn.disabled = false;
  }

  function resetIdle() {
    pendingVersion = null;
    btn.textContent = "check update";
    btn.classList.remove("ready");
    btn.disabled = false;
  }

  // Download + install takes 20-30s and ends with the app restarting itself.
  // Without live feedback the click feels dead — so we drive both the button
  // and the (more visible) status line from the update-progress events.
  async function startInstall() {
    btn.classList.remove("ready");
    btn.textContent = "downloading…";
    btn.disabled = true;
    setStatus("Downloading update…", "working");
    try {
      await invoke("install_update"); // on success the app restarts — no return
    } catch (err) {
      setStatus(`Update failed: ${err}`, "error");
      markAvailable(pendingVersion); // let them retry
    }
  }

  // One handler: click checks for an update, or installs the one we already found.
  btn.addEventListener("click", async () => {
    if (pendingVersion) return startInstall();
    btn.textContent = "checking…";
    btn.disabled = true;
    try {
      const res = await invoke("check_for_update");
      if (res.available) markAvailable(res.version);
      else { btn.textContent = "up to date"; setTimeout(resetIdle, 2500); }
    } catch (err) {
      btn.textContent = "check failed";
      setStatus(`Update check failed: ${err}`, "error");
      setTimeout(resetIdle, 2500);
    }
  });

  // Rust lights the button on its own when a release appears in the background.
  await listen("update-available", (e) => markAvailable(e.payload));
  // Live download progress.
  await listen("update-progress", (e) => {
    const pct = e.payload;
    btn.textContent = `downloading ${pct}%`;
    setStatus(`Downloading update… ${pct}%`, "working");
  });
}

// --- Wiring ---

async function main() {
  // Window controls
  $("#win-min").addEventListener("click", () => getCurrentWindow().minimize());
  $("#win-close").addEventListener("click", () => invoke("hide_to_tray"));
  $("#settings-btn").addEventListener("click", () =>
    showView(currentView === "settings" ? "log" : "settings"));

  // Provider switch
  $("#provider-select").addEventListener("change", async (e) => {
    const provider = e.target.value;
    try {
      await invoke("set_llm_provider", { provider });
      await refreshConfig();
    } catch (err) { setStatus(`Couldn't switch model: ${err}`, "error"); }
  });

  // API key save
  const keyInput = $("#key-input");
  async function saveKey() {
    const key = keyInput.value.trim();
    if (!key) return;
    const provider = $("#provider-select").value;
    try {
      await invoke("set_api_key", { key, provider });
      await refreshConfig();
      setStatus("Key saved", "done");
    } catch (err) { setStatus(`Couldn't save key: ${err}`, "error"); }
  }
  keyInput.addEventListener("keydown", (e) => { if (e.key === "Enter") saveKey(); });
  keyInput.addEventListener("blur", saveKey);
  $("#key-edit").addEventListener("click", () => {
    $("#key-saved").style.display = "none";
    keyInput.style.display = "block";
    keyInput.focus();
  });

  // Debug panel (opened from settings)
  $("#debug-btn").addEventListener("click", async () => {
    $("#debug-content").textContent = await invoke("get_debug_log");
    $("#debug-panel").style.display = "flex";
  });
  $("#debug-close").addEventListener("click", () => { $("#debug-panel").style.display = "none"; });

  // Live events from the correction flow
  await listen("status", (e) => {
    const t = e.payload;
    const kind = t.includes("✓") ? "done" : (t === "Nothing selected" ? "idle" : "working");
    setStatus(t, kind);
  });
  await listen("error", (e) => setStatus(e.payload, "error"));
  await listen("correction", (e) => prependCorrection(e.payload));

  // Initial state
  showView("log");
  const cfg = await refreshConfig();
  const shortcut = pretty(await invoke("get_shortcut"));
  $("#shortcut-display").textContent = shortcut;
  $("#empty-shortcut").textContent = shortcut;
  setupShortcutCapture();
  await setupUpdates();
  await loadHistory();
  if (!cfg.has_api_key) {
    setStatus("Add an API key in settings", "working");
    showView("settings");
    keyInput.focus();
  }
}

main().catch((err) => dlog(`main() failed: ${err}`));

//! Quill settings window.
//!
//! The window is secondary — the product is the global hotkey, which the Rust
//! side owns. This UI just lets you set the model + key and the hotkey, and
//! shows a live status plus a local history of what got corrected.

import { shortcutFromEvent, prettyShortcut } from "./shortcut.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;

const $ = (sel) => document.querySelector(sel);

// Show the hotkey the way each OS writes it (⌃⌥E on Mac, "Ctrl + Alt + E" on
// Windows) while the stored binding stays Tauri's lowercase form.
const IS_MAC = navigator.userAgent.includes("Mac");
const pretty = (raw) => prettyShortcut(raw, IS_MAC);

// Diagnostics without DevTools (disabled in prod builds): goes to the debug log.
function dlog(msg) {
  try { invoke("js_debug_log", { msg: String(msg) }); } catch (_) {}
}

function escapeHtml(s) {
  const d = document.createElement("div");
  d.textContent = s;
  return d.innerHTML;
}

function formatTime(iso) {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
}

// --- Live status line ---

let statusResetTimer = null;
function setStatus(text, kind = "idle") {
  $("#status").className = `status ${kind}`;
  $("#status-text").textContent = text;
  clearTimeout(statusResetTimer);
  // Settle back to "Ready" after a terminal state so the line doesn't lie.
  if (kind === "done" || kind === "error") {
    statusResetTimer = setTimeout(() => setStatus("Ready", "idle"), 4000);
  }
}

// --- History ---

function renderHistory(entries) {
  const list = $("#history-list");
  list.innerHTML = "";
  if (!entries || entries.length === 0) {
    $("#history-empty").style.display = "block";
    return;
  }
  $("#history-empty").style.display = "none";
  for (const e of entries) {
    list.appendChild(historyRow(e));
  }
}

function historyRow(e) {
  const row = document.createElement("div");
  row.className = "history-row";
  const changed = e.original !== e.corrected;
  row.innerHTML = `
    <div class="hr-top">
      <span class="hr-time">${e.ts ? formatTime(e.ts) : ""}</span>
      ${changed ? "" : '<span class="hr-clean">no change</span>'}
    </div>
    <div class="hr-text">${escapeHtml(e.corrected || "")}</div>`;
  if (changed) {
    // Click to reveal the original, so you can see what changed.
    row.title = "click to see the original";
    row.addEventListener("click", () => {
      const t = row.querySelector(".hr-text");
      const showingCorrected = !row.classList.contains("show-original");
      row.classList.toggle("show-original", showingCorrected);
      t.textContent = showingCorrected ? e.original : e.corrected;
      t.classList.toggle("is-original", showingCorrected);
    });
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
  list.insertBefore(historyRow({ ...e, ts: new Date().toISOString() }), list.firstChild);
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

  // Debug panel
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
  const cfg = await refreshConfig();
  $("#shortcut-display").textContent = pretty(await invoke("get_shortcut"));
  setupShortcutCapture();
  await setupUpdates();
  await loadHistory();
  if (!cfg.has_api_key) {
    setStatus("Add an API key to get started", "working");
    keyInput.focus();
  }
}

main().catch((err) => dlog(`main() failed: ${err}`));

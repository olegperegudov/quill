//! Quill main window.
//!
//! The window is secondary — the product is the global hotkey, which the Rust
//! side owns. The body is a chat-style log of what Quill corrected (newest on
//! top); each row shows the polished result and a clock you press-and-hold to
//! peek at the original. A magnifier filters the log live (matching the original
//! text too); settings and updates hide behind the gear, Ribbit-style.

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

// --- Live status (header subtitle, empty at rest) ---

let statusResetTimer = null;
function setStatus(text, kind = "idle") {
  const el = $("#status-detail");
  el.className = `status-detail ${kind}`;
  el.textContent = text || "";
  clearTimeout(statusResetTimer);
  // Anything that isn't an in-flight "working" state is transient — clear it so
  // the header settles back to just "Quill" (the empty subtitle hides itself).
  if (text && kind !== "working") {
    statusResetTimer = setTimeout(() => {
      el.textContent = "";
      el.className = "status-detail idle";
    }, 4000);
  }
}

// --- Search ---

let searchQuery = "";

const matchesQuery = (text, q) => !q || (text || "").toLowerCase().includes(q.toLowerCase());

// Render `text` into `el`, wrapping every occurrence of `q` in <mark>.
function highlightInto(el, text, q) {
  el.textContent = "";
  const query = (q || "").trim();
  if (!query) { el.textContent = text; return; }
  const lc = text.toLowerCase(), lq = query.toLowerCase();
  let i = 0, idx;
  while ((idx = lc.indexOf(lq, i)) !== -1) {
    if (idx > i) el.appendChild(document.createTextNode(text.slice(i, idx)));
    const m = document.createElement("mark");
    m.textContent = text.slice(idx, idx + query.length);
    el.appendChild(m);
    i = idx + query.length;
  }
  if (i < text.length) el.appendChild(document.createTextNode(text.slice(i)));
}

// Paint a row's text for its current state: the original while the clock is
// held, otherwise the corrected text — highlighting the live search query.
function renderRowText(row) {
  const showOrig = row._peeking && row._changed;
  const textEl = row.querySelector(".log-text");
  highlightInto(textEl, showOrig ? row._orig : row._corr, searchQuery);
  textEl.classList.toggle("is-original", showOrig);
}

// Live filter. A row stays if the query hits the corrected OR the original
// text. When it only hits the (hidden) original, the clock lights up so you
// know to hold it — and holding reveals the highlighted match.
function applySearch() {
  const q = searchQuery.trim();
  for (const row of $("#history-list").children) {
    const hitCorr = matchesQuery(row._corr, q);
    const hitOrig = row._changed && matchesQuery(row._orig, q);
    row.style.display = (!q || hitCorr || hitOrig) ? "" : "none";
    if (row._clock) row._clock.classList.toggle("search-hit", !!q && hitOrig);
    renderRowText(row);
  }
}

function openSearch() {
  if (currentView !== "log") showView("log");
  $("#search-popup").style.display = "block";
  $("#search-input").focus();
}

function closeSearch() {
  const p = $("#search-popup");
  if (!p) return;
  p.style.display = "none";
  const inp = $("#search-input");
  if (inp) inp.value = "";
  searchQuery = "";
  applySearch();
}

// --- View switching (log <-> settings) ---

let currentView = "log";
function showView(name) {
  currentView = name;
  $("#log-view").style.display = name === "log" ? "flex" : "none";
  $("#settings-panel").style.display = name === "settings" ? "flex" : "none";
  $("#settings-btn").classList.toggle("active", name === "settings");
  if (name !== "log") closeSearch();
}

// --- Log ---

function renderHistory(entries) {
  const list = $("#history-list");
  const empty = $("#history-empty");
  list.innerHTML = "";
  if (!entries || entries.length === 0) {
    list.style.display = "none";
    empty.style.display = "flex";
    return;
  }
  list.style.display = "";
  empty.style.display = "none";
  for (const e of entries) list.appendChild(logRow(e));
  applySearch();
}

// One correction as a chat-style row: time, the polished text, and — when the
// text actually changed — a clock you hold down to reveal the original.
function logRow(e) {
  const row = document.createElement("div");
  row.className = "log-entry";
  row._orig = e.original || "";
  row._corr = e.corrected || "";
  row._changed = e.original !== e.corrected;
  row._peeking = false;

  const time = document.createElement("span");
  time.className = "log-time";
  time.textContent = e.ts ? formatTime(e.ts) : "";

  const text = document.createElement("span");
  text.className = "log-text";

  row.append(time, text);

  if (row._changed) {
    const clock = document.createElement("button");
    clock.className = "log-clock";
    clock.title = "hold to see the original";
    clock.tabIndex = -1;
    clock.innerHTML = CLOCK_SVG;
    clock.addEventListener("pointerdown", (ev) => {
      ev.preventDefault();
      // Capture the pointer so release restores even if the cursor drifts off.
      try { clock.setPointerCapture(ev.pointerId); } catch (_) {}
      row._peeking = true;
      renderRowText(row);
    });
    const release = () => { row._peeking = false; renderRowText(row); };
    clock.addEventListener("pointerup", release);
    clock.addEventListener("pointercancel", release);
    row._clock = clock;
    row.appendChild(clock);
  } else {
    const clean = document.createElement("span");
    clean.className = "log-clean";
    clean.textContent = "already clean";
    row.appendChild(clean);
  }
  renderRowText(row);
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
  list.style.display = "";
  list.insertBefore(logRow({ ...e, ts: new Date().toISOString() }), list.firstChild);
  applySearch();
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

// --- Update flow (Ribbit-style: button lives in settings, the gear glows when
//     an update is waiting so you notice it from the log) ---

async function setupUpdates() {
  $("#version").textContent = "v" + (await invoke("get_current_version"));
  const btn = $("#update-btn");
  const gear = $("#settings-btn");

  function setIdle() {
    btn.textContent = "check update";
    btn.classList.remove("update-available");
    gear.classList.remove("update-available");
    btn.disabled = false;
    btn.onclick = check;
  }

  // Arm the button (and light the gear) to install `version` on the next click.
  function arm(version) {
    btn.textContent = `update to v${version}`;
    btn.classList.add("update-available");
    gear.classList.add("update-available");
    btn.disabled = false;
    btn.onclick = () => install(version);
  }

  async function check() {
    btn.textContent = "checking…";
    btn.disabled = true;
    try {
      const res = await invoke("check_for_update");
      if (res.available) arm(res.version);
      else { btn.textContent = "up to date"; setTimeout(setIdle, 2500); }
    } catch (err) {
      btn.textContent = "check failed";
      setStatus(`Update check failed: ${err}`, "error");
      setTimeout(setIdle, 2500);
    }
  }

  // Download + install takes 20-30s and ends with the app restarting itself.
  // The percentage rides on the button only (Ribbit-style) — one place, not two.
  async function install(version) {
    btn.textContent = "downloading…";
    btn.disabled = true;
    try {
      await invoke("install_update"); // on success the app restarts — no return
    } catch (err) {
      btn.textContent = "update failed";
      setTimeout(() => arm(version), 2500); // let them retry
    }
  }

  setIdle();
  // Rust finds a release in the background → light the gear + arm install.
  await listen("update-available", (e) => arm(e.payload));
  await listen("update-progress", (e) => { btn.textContent = `downloading ${e.payload}%`; });
}

// --- Wiring ---

async function main() {
  // Window controls
  $("#win-min").addEventListener("click", () => getCurrentWindow().minimize());
  $("#win-close").addEventListener("click", () => invoke("hide_to_tray"));
  $("#settings-btn").addEventListener("click", () =>
    showView(currentView === "settings" ? "log" : "settings"));

  // Search
  $("#search-btn").addEventListener("click", () => {
    if ($("#search-popup").style.display === "none") openSearch();
    else closeSearch();
  });
  $("#search-input").addEventListener("input", (e) => { searchQuery = e.target.value; applySearch(); });
  $("#search-input").addEventListener("keydown", (e) => {
    if (e.key === "Escape") { e.preventDefault(); closeSearch(); }
  });

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
  // Opened from the chat's gear — land on settings, not the history.
  await listen("show-settings", () => showView("settings"));

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

//! Quill settings — an in-window overlay, not a separate window.
//!
//! The app has one face: the chat (editor.js). The gear in its titlebar flips
//! this panel over the chat (Ribbit-style), the way Ribbit's gear swaps its log
//! for settings — nothing opens a second window. This module owns what lives
//! *inside* the panel (model, API key, hotkey, updates, debug log) and reports
//! transient status in the panel header. editor.js owns showing/hiding it.

import { shortcutFromEvent, prettyShortcut } from "./shortcut.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel) => document.querySelector(sel);

// Show the hotkey the way each OS writes it (⌃⌥E on Mac, "Ctrl + Alt + E" on
// Windows) while the stored binding stays Tauri's lowercase form.
const IS_MAC = navigator.userAgent.includes("Mac");
const pretty = (raw) => prettyShortcut(raw, IS_MAC);

function dlog(msg) {
  try { invoke("js_debug_log", { msg: String(msg) }); } catch (_) {}
}

// --- Live status (settings header subtitle, empty at rest) ---

let statusResetTimer = null;
function setStatus(text, kind = "idle") {
  const el = $("#status-detail");
  if (!el) return;
  el.className = `status-detail ${kind}`;
  el.textContent = text || "";
  clearTimeout(statusResetTimer);
  // Anything that isn't an in-flight "working" state is transient — clear it so
  // the header settles back to just "Settings".
  if (text && kind !== "working") {
    statusResetTimer = setTimeout(() => {
      el.textContent = "";
      el.className = "status-detail idle";
    }, 4000);
  }
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

  // The `.capturing` class is the shared signal: editor.js checks it so its Esc
  // handler defers to us (Esc here cancels capture instead of closing settings).
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
      setStatus(`Hotkey: ${pretty(shortcut)}`, "done");
    } catch (err) {
      disp.textContent = pretty(await invoke("get_shortcut"));
      setStatus(`Couldn't set hotkey: ${err}`, "error");
    }
  });
}

// --- Update flow (Ribbit-style: the button shows the percent and installs;
//     the chat's gear is what glows when one is waiting) ---

async function setupUpdates() {
  $("#version").textContent = "v" + (await invoke("get_current_version"));
  const btn = $("#update-btn");

  function setIdle() {
    btn.textContent = "check update";
    btn.classList.remove("update-available");
    btn.disabled = false;
    btn.onclick = check;
  }

  // Arm the button to install `version` on the next click.
  function arm(version) {
    btn.textContent = `update to v${version}`;
    btn.classList.add("update-available");
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
  // Rust finds a release in the background → arm install (the chat's gear glows).
  await listen("update-available", (e) => arm(e.payload));
  await listen("update-progress", (e) => { btn.textContent = `downloading ${e.payload}%`; });
}

// --- Init (called once by editor.js when the window loads) ---

export async function initSettings() {
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

  // (The Debug log button/back are wired in editor.js — it owns view switching.)

  // Initial state
  const cfg = await refreshConfig();
  $("#shortcut-display").textContent = pretty(await invoke("get_shortcut"));
  setupShortcutCapture();
  await setupUpdates();
  return cfg;
}

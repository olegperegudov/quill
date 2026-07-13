//! Quill settings — an in-window overlay, not a separate window.
//!
//! The app has one face: the chat (editor.js). The gear in its titlebar flips
//! this panel over the chat (Ribbit-style), the way Ribbit's gear swaps its log
//! for settings — nothing opens a second window. This module owns what lives
//! *inside* the panel (model, API key, hotkey, updates, debug log) and reports
//! transient status in the panel header. editor.js owns showing/hiding it.

import { shortcutFromEvent, prettyShortcut } from "./shortcut.js";

const { invoke } = window.__TAURI__.core;

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

// --- Provider stack ---
//
// An ordered list of providers rendered as compact cards. The top entry runs
// first; the backend switches to the next on repeated rate-limits/outages and
// snaps back after the cooldown. Order = priority, changed with ↑/↓. The section
// is data-driven off get_config, so any mutation just re-renders it.

let catalog = null; // [{name,label,default_model}] — fetched once

async function loadCatalog() {
  if (!catalog) {
    try { catalog = await invoke("list_provider_catalog"); }
    catch (err) { dlog(`list_provider_catalog failed: ${err}`); catalog = []; }
  }
  return catalog;
}

function miniBtn(label, disabled, title, onClick) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "provider-mini-btn";
  b.textContent = label;
  b.title = title;
  b.disabled = !!disabled;
  if (!disabled) b.addEventListener("click", onClick);
  return b;
}

function fieldRow(label, value, placeholder, onChange) {
  const row = document.createElement("div");
  row.className = "provider-field";
  const l = document.createElement("span");
  l.className = "provider-field-label";
  l.textContent = label;
  const i = document.createElement("input");
  i.type = "text";
  i.className = "provider-input mono";
  i.value = value || "";
  i.placeholder = placeholder;
  i.autocomplete = "off";
  i.spellcheck = false;
  i.addEventListener("change", () => onChange(i.value.trim()));
  row.append(l, i);
  return row;
}

// The key row: an input while unset, a "saved ✓ / edit" chip once stored.
function keyRow(entry) {
  const row = document.createElement("div");
  row.className = "provider-field";
  const label = document.createElement("span");
  label.className = "provider-field-label";
  label.textContent = "key";

  const input = document.createElement("input");
  input.type = "password";
  input.className = "provider-input";
  input.placeholder = "paste token";
  input.autocomplete = "off";

  const chip = document.createElement("span");
  chip.className = "key-status";
  chip.innerHTML = '<span class="key-saved-check">✓</span> saved <a class="link">edit</a>';

  const show = (saved) => {
    chip.style.display = saved ? "inline-flex" : "none";
    input.style.display = saved ? "none" : "block";
    input.value = "";
  };
  chip.querySelector(".link").addEventListener("click", () => { show(false); input.focus(); });

  async function save() {
    const key = input.value.trim();
    if (!key) return;
    try {
      await invoke("set_provider_key", { id: entry.id, key });
      show(true);
      setStatus("Key saved", "done");
    } catch (err) { setStatus(`Couldn't save key: ${err}`, "error"); }
  }
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") save(); });
  input.addEventListener("blur", save);

  show(!!entry.has_key);
  row.append(label, input, chip);
  return row;
}

// One entry: header (name + reorder/remove) over endpoint, model and key.
function providerCard(entry, index, total) {
  const card = document.createElement("div");
  card.className = "provider-card";

  const head = document.createElement("div");
  head.className = "provider-head";
  const name = document.createElement("span");
  name.className = "provider-name";
  name.textContent = entry.label || "custom";
  if (index === 0) {
    const tag = document.createElement("span");
    tag.className = "provider-primary-tag";
    tag.textContent = "first";
    name.appendChild(tag);
  }
  head.appendChild(name);

  const ctrls = document.createElement("div");
  ctrls.className = "provider-ctrls";
  ctrls.appendChild(miniBtn("↑", index === 0, "Move up (runs earlier)", async () => {
    await invoke("move_provider", { id: entry.id, up: true });
    renderStack();
  }));
  ctrls.appendChild(miniBtn("↓", index === total - 1, "Move down", async () => {
    await invoke("move_provider", { id: entry.id, up: false });
    renderStack();
  }));
  const del = miniBtn("✕", false, "Remove", async () => {
    await invoke("remove_provider", { id: entry.id });
    renderStack();
  });
  del.classList.add("provider-del");
  ctrls.appendChild(del);
  head.appendChild(ctrls);
  card.appendChild(head);

  card.appendChild(fieldRow("endpoint", entry.url, "https://…/v1/chat/completions", (v) =>
    invoke("set_provider_field", { id: entry.id, field: "url", value: v })
  ));
  card.appendChild(fieldRow("model", entry.model, "model id", (v) =>
    invoke("set_provider_field", { id: entry.id, field: "model", value: v })
  ));
  card.appendChild(keyRow(entry));
  return card;
}

async function renderStack() {
  const container = $("#provider-stack");
  const cfg = await invoke("get_config");
  const entries = cfg.providers || [];
  container.innerHTML = "";

  // Surface an active fallback: an outage the user can't see is an outage they
  // blame on Quill.
  const st = cfg.fallback_state;
  if (st) {
    const line = document.createElement("div");
    line.className = "fallback-status";
    const mins = Math.max(1, Math.ceil((st.remaining_secs || 0) / 60));
    const active = entries[st.active];
    const who = active ? (active.label || active.url) : `#${st.active + 1}`;
    line.textContent = `⚡ running on ${who} (#${st.active + 1} of ${st.total}) · first choice retried in ~${mins} min`;
    container.appendChild(line);
  }

  entries.forEach((e, i) => container.appendChild(providerCard(e, i, entries.length)));

  const add = document.createElement("div");
  add.className = "provider-add";
  const sel = document.createElement("select");
  sel.innerHTML = '<option value="" disabled selected>+ add model</option>';
  for (const p of await loadCatalog()) {
    const opt = document.createElement("option");
    opt.value = p.name;
    opt.textContent = p.label;
    sel.appendChild(opt);
  }
  sel.insertAdjacentHTML("beforeend", '<option value="custom">custom…</option>');
  sel.addEventListener("change", async () => {
    if (!sel.value) return;
    try { await invoke("add_provider", { provider: sel.value }); renderStack(); }
    catch (err) { setStatus(`Couldn't add model: ${err}`, "error"); }
  });
  add.appendChild(sel);
  container.appendChild(add);

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

// Updating lives entirely in the menu-bar menu — the window only names the
// running version.
async function setupUpdates() {
  $("#version").textContent = "v" + (await invoke("get_current_version"));
}

// --- Init (called once by editor.js when the window loads) ---

export async function initSettings() {
  // (The Debug log button/back are wired in editor.js — it owns view switching.)

  // Initial state
  const cfg = await renderStack();

  // Fallback knobs: how stubborn we are with a failing model, and how long
  // before the first choice gets another chance.
  const fbThreshold = $("#fb-threshold");
  const fbCooldown = $("#fb-cooldown");
  fbThreshold.value = cfg.fallback_threshold;
  fbCooldown.value = cfg.fallback_cooldown_mins;
  fbThreshold.addEventListener("change", () =>
    invoke("set_fallback_threshold", { value: parseInt(fbThreshold.value, 10) || 2 })
  );
  fbCooldown.addEventListener("change", () =>
    invoke("set_fallback_cooldown", { minutes: parseInt(fbCooldown.value, 10) || 60 })
  );

  $("#shortcut-display").textContent = pretty(await invoke("get_shortcut"));
  setupShortcutCapture();
  await setupUpdates();
  return cfg;
}

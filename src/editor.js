//! Quill editor window — review a correction, tweak it, then apply it.
//!
//! The Rust side opens this window over the captured selection (event
//! `editor:open` carries the original text) and remembers which app was
//! frontmost. Here we run the correction, let the user edit, and on "Apply"
//! ask Rust to re-activate that app and type the final text back over it.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (sel) => document.querySelector(sel);
const textarea = $("#text");

// Diagnostics without DevTools (disabled in prod) — goes to the shared debug log.
function dlog(msg) {
  try { invoke("js_debug_log", { msg: String(msg) }); } catch (_) {}
}

let original = "";
// Monotonic id so a slow correction that returns after a newer one is ignored
// (already useful now; load-bearing once live re-checking lands).
let reqId = 0;

function setStatus(text, kind = "idle") {
  $("#status").className = `status ${kind}`;
  $("#status-text").textContent = text;
}

async function check() {
  const sent = textarea.value;
  if (!sent.trim()) { setStatus("Пусто", "idle"); return; }
  const id = ++reqId;
  setStatus("Проверяю…", "working");
  try {
    const corrected = await invoke("editor_correct", { text: sent });
    if (id !== reqId) return; // superseded by a newer check
    if (corrected === sent) {
      setStatus("Уже чисто", "done");
    } else {
      textarea.value = corrected;
      setStatus("Готово — проверь и применяй", "done");
    }
  } catch (err) {
    if (id !== reqId) return;
    setStatus(String(err), "error");
  }
}

async function apply() {
  try {
    await invoke("apply_correction", { original, text: textarea.value });
  } catch (err) {
    setStatus(String(err), "error");
  }
}

async function cancel() {
  try { await invoke("close_editor"); } catch (_) {}
}

// Rust hands us the captured text and shows the window.
listen("editor:open", (e) => {
  original = e.payload || "";
  textarea.value = original;
  textarea.focus();
  const end = textarea.value.length;
  textarea.setSelectionRange(end, end);
  check();
});

$("#apply").addEventListener("click", apply);
$("#recheck").addEventListener("click", check);
$("#cancel").addEventListener("click", cancel);
$("#close").addEventListener("click", cancel);

window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") { e.preventDefault(); cancel(); return; }
  // ⌘⏎ / Ctrl+⏎ applies. Plain Enter stays a newline in the textarea.
  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); apply(); }
});

dlog("editor window ready");

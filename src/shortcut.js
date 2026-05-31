//! Pure shortcut helpers, kept out of main.js so they unit-test without a DOM:
//! - shortcutFromEvent: a keydown event → Tauri shortcut string ("ctrl+alt+e")
//! - prettyShortcut: that stored string → a platform-correct label (⌃⌥E on Mac,
//!   "Ctrl + Alt + E" on Windows)

const MODIFIER_KEYS = ["control", "alt", "shift", "meta", "os"];

export function shortcutFromEvent(e) {
  const parts = [];
  if (e.ctrlKey) parts.push("ctrl");
  if (e.altKey) parts.push("alt");
  if (e.shiftKey) parts.push("shift");
  if (e.metaKey) parts.push("cmd");

  const k = (e.key || "").toLowerCase();
  const isModifier = MODIFIER_KEYS.includes(k);
  if (!isModifier && k) {
    parts.push(k === " " ? "space" : k);
  }

  // A usable global shortcut needs a non-modifier key plus at least one
  // modifier — otherwise a stray letter press would hijack the binding.
  return { parts, complete: !isModifier && !!k && parts.length >= 2 };
}

// Mac and Windows name the same physical keys differently. The binding we store
// is always Tauri's lowercase form ("ctrl+alt+e"); this is display only — so the
// user sees ⌃⌥E on a Mac and "Ctrl + Alt + E" on Windows for the same hotkey.
const MAC_SYMBOLS = { ctrl: "⌃", control: "⌃", alt: "⌥", option: "⌥", shift: "⇧", cmd: "⌘", command: "⌘", super: "⌘", meta: "⌘" };
const WIN_NAMES = { ctrl: "Ctrl", control: "Ctrl", alt: "Alt", shift: "Shift", cmd: "Win", command: "Win", super: "Win", meta: "Win" };

export function prettyShortcut(raw, isMac) {
  const parts = String(raw).split("+").map((p) => p.trim().toLowerCase()).filter(Boolean);
  if (isMac) {
    // Mac convention: bare glyphs, no separators (⌃⌥E).
    return parts.map((p) => MAC_SYMBOLS[p] || (p === "space" ? "Space" : p.toUpperCase())).join("");
  }
  return parts.map((p) => WIN_NAMES[p] || (p === "space" ? "Space" : p.toUpperCase())).join(" + ");
}

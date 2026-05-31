//! Pure helper: turn a keydown event into a Tauri global-shortcut string
//! (e.g. "ctrl+alt+e"). Kept out of main.js so it can be unit-tested without a
//! DOM — it only reads the boolean modifier flags and `.key`.

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

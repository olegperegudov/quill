//! What a key press means in Quill's window.
//!
//! The window is driven from the keyboard, and the same key means different
//! things depending on one thing: whether the field is holding text. Empty, it
//! is a list and the arrows walk it; holding something, it is material and the
//! arrows belong to the caret. Keeping that decision here — pure, no DOM —
//! leaves editor.js with only the carrying out, and lets the rules be read (and
//! tested) in one place instead of inferred from a handler.

/**
 * @param {{key: string, shiftKey?: boolean}} event
 * @param {{view: string, hasText: boolean, working: boolean, capturing?: boolean}} state
 * @returns {null|"back"|"send"|"copy-close"|"pending-close"|"stop"|"clear"|"close"|"prev"|"next"|"was"|"now"}
 */
export function keyAction(event, state) {
  const { key, shiftKey } = event;
  const { view, hasText, working, capturing } = state;

  // Settings and the debug log answer to one key, and not while a shortcut is
  // being captured — there Esc cancels the capture instead.
  if (view !== "chat") return key === "Escape" && !capturing ? "back" : null;

  if (key === "Enter" && !shiftKey) {
    // Pressed while the model is still reading: a promise to take whatever
    // comes back, so the window can go now.
    if (working) return "pending-close";
    return hasText ? "send" : "copy-close";
  }

  if (key === "Escape") {
    if (working) return "stop";
    return hasText ? "clear" : "close";
  }

  // Text in the field means the arrows are moving the caret through it.
  if (hasText) return null;

  if (key === "ArrowUp") return "prev";
  if (key === "ArrowDown") return "next";
  if (key === "ArrowLeft") return "was";
  if (key === "ArrowRight") return "now";
  return null;
}

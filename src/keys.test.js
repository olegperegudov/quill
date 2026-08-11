//! The keyboard contract. Every rule here is invisible from the outside and
//! silent when broken: an arrow that walks the history instead of the caret
//! eats an edit to pasted text, and an Enter that copies instead of promising
//! leaves the user with a stale clipboard and a closed window.
import { describe, it, expect } from "vitest";
import { keyAction } from "./keys.js";

const chat = (over = {}) => ({ view: "chat", hasText: false, working: false, ...over });
const press = (key, state, shiftKey = false) => keyAction({ key, shiftKey }, state);

describe("keys — the field is empty", () => {
  it("walks the history with the arrows", () => {
    expect(press("ArrowUp", chat())).toBe("prev");
    expect(press("ArrowDown", chat())).toBe("next");
  });

  it("shows what was dictated on the left, the finished text on the right", () => {
    expect(press("ArrowLeft", chat())).toBe("was");
    expect(press("ArrowRight", chat())).toBe("now");
  });

  it("copies the selected entry and closes", () => {
    expect(press("Enter", chat())).toBe("copy-close");
  });

  it("closes on Esc", () => {
    expect(press("Escape", chat())).toBe("close");
  });
});

describe("keys — the field holds something", () => {
  const typing = chat({ hasText: true });

  it("corrects it on Enter, and leaves Shift+Enter to make a line", () => {
    expect(press("Enter", typing)).toBe("send");
    expect(press("Enter", typing, true)).toBe(null);
  });

  it("leaves the arrows to the caret — pasted text stays editable", () => {
    for (const key of ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]) {
      expect(press(key, typing)).toBe(null);
    }
  });

  it("clears the field on Esc instead of closing the window", () => {
    expect(press("Escape", typing)).toBe("clear");
  });
});

describe("keys — a correction is in flight", () => {
  const busy = chat({ working: true });

  it("takes Enter as a promise: close now, clipboard when it lands", () => {
    expect(press("Enter", busy)).toBe("pending-close");
    // Even with the next text already typed — the promise is about the answer
    // being written, not about the field.
    expect(press("Enter", chat({ working: true, hasText: true }))).toBe("pending-close");
  });

  it("stops it on Esc rather than closing the window", () => {
    expect(press("Escape", busy)).toBe("stop");
  });
});

describe("keys — settings and the debug log", () => {
  it("answer to Esc and nothing else", () => {
    expect(press("Escape", chat({ view: "settings" }))).toBe("back");
    expect(press("Enter", chat({ view: "settings" }))).toBe(null);
    expect(press("ArrowUp", chat({ view: "debug" }))).toBe(null);
  });

  it("leave Esc to a live shortcut capture, which cancels on it", () => {
    expect(press("Escape", chat({ view: "settings", capturing: true }))).toBe(null);
  });
});

import { describe, it, expect } from "vitest";
import { shortcutFromEvent, prettyShortcut } from "./shortcut.js";

const ev = (over) => ({
  ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, key: "", ...over,
});

describe("shortcutFromEvent", () => {
  it("builds modifier + letter combos", () => {
    const r = shortcutFromEvent(ev({ ctrlKey: true, altKey: true, key: "E" }));
    expect(r.parts).toEqual(["ctrl", "alt", "e"]);
    expect(r.complete).toBe(true);
  });

  it("maps space and meta", () => {
    const r = shortcutFromEvent(ev({ metaKey: true, shiftKey: true, key: " " }));
    expect(r.parts).toEqual(["shift", "cmd", "space"]);
    expect(r.complete).toBe(true);
  });

  it("is incomplete while only modifiers are held", () => {
    const r = shortcutFromEvent(ev({ ctrlKey: true, key: "Control" }));
    expect(r.complete).toBe(false);
  });

  it("rejects a bare letter with no modifier", () => {
    const r = shortcutFromEvent(ev({ key: "e" }));
    expect(r.complete).toBe(false);
  });
});

describe("prettyShortcut", () => {
  it("renders Mac glyphs with no separators", () => {
    expect(prettyShortcut("ctrl+alt+e", true)).toBe("⌃⌥E");
    expect(prettyShortcut("cmd+shift+space", true)).toBe("⌘⇧Space");
  });

  it("renders Windows names with separators", () => {
    expect(prettyShortcut("ctrl+alt+e", false)).toBe("Ctrl + Alt + E");
    expect(prettyShortcut("ctrl+shift+space", false)).toBe("Ctrl + Shift + Space");
  });

  it("same stored binding, different label per platform", () => {
    const raw = "ctrl+alt+e";
    expect(prettyShortcut(raw, true)).not.toBe(prettyShortcut(raw, false));
  });
});

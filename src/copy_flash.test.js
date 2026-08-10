//! Guard for the copy signal — a click on a bubble puts the correction in the
//! clipboard and the whole text flashes green (Ribbit's signal). Both halves are
//! invisible to any other test: the flash is CSS, and the thing that gets copied
//! is one argument at one call site. Getting either wrong is silent — the user
//! pastes diff markup, or pastes and never learns whether it worked.
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";

const css = readFileSync(new URL("./editor.css", import.meta.url), "utf8");
const js = readFileSync(new URL("./editor.js", import.meta.url), "utf8");

const block = (selector) => {
  const start = css.indexOf(selector + " {");
  if (start === -1) return "";
  return css.slice(start, css.indexOf("}", start));
};

describe("copy flash", () => {
  it("paints the whole text, not a badge in the corner", () => {
    const flash = block(".bubble.copied");
    expect(flash).toMatch(/color:\s*var\(--good\)/);
    expect(flash).toMatch(/background:\s*rgba\(74,\s*222,\s*128/);
  });

  it("pulls the edit marks into the flash so the bubble reads as one text", () => {
    const marks = block(".bubble.copied ins,\n.bubble.copied del");
    expect(marks).toMatch(/color:\s*var\(--good\)/);
    expect(marks).toMatch(/background:\s*transparent/);
  });

  it("lands at once and fades back out", () => {
    expect(block(".bubble.copied")).toMatch(/transition:\s*none/);
    expect(block(".bubble")).toMatch(/transition:[^;]*background-color/);
  });

  it("copies the correction, never the markup", () => {
    // `text` is the plain correction; the bubble's own content carries <ins>/<del>.
    expect(js).toMatch(/invoke\("copy_to_clipboard",\s*\{\s*text\s*\}\)/);
    expect(js).not.toMatch(/copy_to_clipboard[\s\S]{0,80}(innerHTML|textContent)/);
  });
});

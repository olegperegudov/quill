//! Guard for the copy signal — Enter (or a click) puts the correction in the
//! clipboard and the entry's meta says so. Both halves are invisible to any
//! other test: the word is CSS, and the thing that gets copied is one argument
//! at one call site. Getting either wrong is silent — the user pastes diff
//! markup, or pastes and never learns whether it worked.
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";

const css = readFileSync(new URL("./editor.css", import.meta.url), "utf8");
const js = readFileSync(new URL("./editor.js", import.meta.url), "utf8");

const block = (selector) => {
  const start = css.indexOf(selector + " {");
  if (start === -1) return "";
  return css.slice(start, css.indexOf("}", start));
};

describe("copy", () => {
  it("says so in the entry's own meta, in the colour that means done", () => {
    expect(block(".meta .copied")).toMatch(/color:\s*var\(--ok\)/);
  });

  it("copies the correction, never the markup", () => {
    // `e.corrected` is the plain text; the marks live in <ins>/<del> nodes built
    // by paintMarks and are never handed to the clipboard.
    expect(js).toMatch(/invoke\("copy_to_clipboard",\s*\{\s*text:\s*e\.corrected\s*\}\)/);
    expect(js).not.toMatch(/copy_to_clipboard[\s\S]{0,80}(innerHTML|textContent)/);
  });

  it("closes the window after copying with the key, but not after a click", () => {
    // The mouse is used while looking at the list; taking the window away
    // mid-look is rude. The key press is the deliberate end of the job.
    expect(js).toMatch(/copySelected\(\{\s*close:\s*false\s*\}\)/);
    expect(js).toMatch(/if \(close\) invoke\("close_editor"\)/);
  });
});

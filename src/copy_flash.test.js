//! Guard for the copy signal — a click on the clean sheet puts the correction in
//! the clipboard and the whole sheet flashes (Ribbit's signal). Both halves are
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
    const flash = block(".sheet.copied");
    expect(flash).toMatch(/color:\s*var\(--stamp\)/);
    expect(flash).toMatch(/background:\s*var\(--stamp-wash\)/);
  });

  it("flashes the sheet's own label with it, so the block answers as one", () => {
    expect(block(".sheet.copied .slug")).toMatch(/color:\s*var\(--stamp\)/);
  });

  it("lands at once and fades back out", () => {
    expect(block(".sheet.copied")).toMatch(/transition:\s*none/);
    expect(block(".sheet")).toMatch(/transition:[^;]*background-color/);
  });

  it("copies the correction, never the markup", () => {
    // `text` is the plain correction; the transcript above carries the <ins>/<del>.
    expect(js).toMatch(/invoke\("copy_to_clipboard",\s*\{\s*text\s*\}\)/);
    expect(js).not.toMatch(/copy_to_clipboard[\s\S]{0,80}(innerHTML|textContent)/);
  });
});

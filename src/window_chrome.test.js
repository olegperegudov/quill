//! Guard for the window "skin" — the borderless, transparent window means the page
//! itself is what the user sees as the app. If the document can scroll or bounce,
//! a two-finger swipe drags that skin around and exposes the desktop behind it
//! (looked like white gaps along the edges). The bug only reproduces on a real
//! macOS build, so the CSS that prevents it is pinned here instead.
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";

const css = readFileSync(new URL("./editor.css", import.meta.url), "utf8");

const block = (selector) => {
  const start = css.indexOf(selector + " {");
  if (start === -1) return "";
  return css.slice(start, css.indexOf("}", start));
};

describe("window chrome", () => {
  it("pins the root so the document never scrolls or bounces", () => {
    const root = block("html, body");
    expect(root).toMatch(/height:\s*100%/);
    expect(root).toMatch(/overflow:\s*hidden/);
    expect(root).toMatch(/overscroll-behavior:\s*none/);
    expect(root).toMatch(/background:\s*transparent/);
  });

  it("keeps every scroll region's overscroll to itself", () => {
    for (const selector of [".log", ".view-panel", "#debug-content"]) {
      expect(block(selector)).toMatch(/overscroll-behavior:\s*contain/);
    }
  });
});

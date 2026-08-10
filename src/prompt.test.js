//! Guard for the prompt panel — the instruction is the user's to rewrite, the
//! two rules Quill appends are not. Both halves live in markup and one call
//! site, where nothing else would notice them going wrong: an editable guard
//! would let a pasted prompt switch off the injection protection, and a reset
//! that sends the shipped text instead of an empty string would freeze today's
//! wording into the user's config for good.
import { readFileSync } from "node:fs";
import { describe, it, expect } from "vitest";

const html = readFileSync(new URL("./editor.html", import.meta.url), "utf8");
const settings = readFileSync(new URL("./settings.js", import.meta.url), "utf8");
const editor = readFileSync(new URL("./editor.js", import.meta.url), "utf8");

describe("the prompt panel", () => {
  it("gives the instruction a box and the guard a statement, not a second box", () => {
    expect(html).toMatch(/<textarea id="prompt-input"/);
    expect(html).toMatch(/<p class="prompt-guard" id="prompt-guard"/);
    // One editable field in the block: a guard the user can type into is no guard.
    const box = html.slice(html.indexOf('class="prompt-box"'), html.indexOf("Models: an ordered stack"));
    expect(box.match(/<textarea/g)).toHaveLength(1);
    expect(box).not.toMatch(/<input/);
  });

  it("saves what the user wrote, and only that", () => {
    expect(settings).toMatch(/invoke\("set_prompt", \{ instruction: value \}\)/);
  });

  it("resets by sending nothing, so the shipped wording stays Quill's to change", () => {
    expect(settings).toMatch(/invoke\("set_prompt", \{ instruction: "" \}\)/);
  });

  it("measures the box when settings open, since a hidden panel has no height", () => {
    expect(settings).toMatch(/export function refitPrompt/);
    expect(editor).toMatch(/if \(name === "settings"\) refitPrompt\(\)/);
  });
});

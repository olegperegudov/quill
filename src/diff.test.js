//! The reply bubble only earns its highlighting if the marks are the real edits:
//! everything marked "ins" must be in the correction, everything "del" must be in
//! the original, and an untouched text must come back with nothing marked at all.
import { describe, it, expect } from "vitest";
import { diffWords, tokenize, MAX_TOKENS } from "./diff.js";

const join = (ops, ...types) => ops.filter((o) => types.includes(o.type)).map((o) => o.text).join("");
const marked = (ops, type) => ops.filter((o) => o.type === type).map((o) => o.text);

describe("tokenize", () => {
  it("keeps whitespace and punctuation as tokens of their own", () => {
    expect(tokenize("hi, there")).toEqual(["hi", ",", " ", "there"]);
  });

  it("holds an apostrophe inside the word", () => {
    expect(tokenize("don't")).toEqual(["don't"]);
  });
});

describe("diffWords", () => {
  // The correction is what the user copies out, so "same"+"ins" must be it exactly.
  // The original is not rebuildable from the ops — case-only edits deliberately
  // drop their struck-through half (see below) — but every real removal is marked.
  it("rebuilds the correction from the ops and marks every real removal", () => {
    const before = "i think we should of shipped this yesterday, their is no reason to wait";
    const after = "I think we should have shipped this yesterday; there is no reason to wait.";
    const ops = diffWords(before, after);
    expect(join(ops, "same", "ins")).toBe(after);
    expect(marked(ops, "del")).toEqual(["of", ",", "their"]);
  });

  it("marks a capitalised word as added, not as a swap of one letter", () => {
    const ops = diffWords("в общем всё", "В общем всё");
    expect(marked(ops, "del")).toEqual([]);
    expect(marked(ops, "ins")).toEqual(["В"]);
  });

  it("marks a replaced word both ways", () => {
    const ops = diffWords("their is no reason", "there is no reason");
    expect(marked(ops, "del")).toEqual(["their"]);
    expect(marked(ops, "ins")).toEqual(["there"]);
  });

  it("marks an inserted comma without dragging its neighbours in", () => {
    const ops = diffWords("в общем давай сделаем так", "В общем, давай сделаем так");
    expect(marked(ops, "ins")).toContain(",");
    expect(join(ops, "same")).toContain(" давай сделаем так");
  });

  it("marks nothing when the correction changed nothing", () => {
    const ops = diffWords("Спасибо за правки, всё учёл.", "Спасибо за правки, всё учёл.");
    expect(ops).toEqual([{ type: "same", text: "Спасибо за правки, всё учёл." }]);
  });

  it("diffs a long text cheaply — untouched head and tail never reach the table", () => {
    const filler = "word ".repeat(MAX_TOKENS);
    const ops = diffWords(`${filler}teh end`, `${filler}the end`);
    expect(marked(ops, "del")).toEqual(["teh"]);
    expect(marked(ops, "ins")).toEqual(["the"]);
  });

  it("gives up on a text too large to diff, instead of hanging on it", () => {
    const before = Array.from({ length: MAX_TOKENS + 1 }, (_, i) => `a${i}`).join(" ");
    const after = Array.from({ length: MAX_TOKENS + 1 }, (_, i) => `b${i}`).join(" ");
    expect(diffWords(before, after)).toBeNull();
  });
});

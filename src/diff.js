//! Word-level diff between what you sent and what came back.
//!
//! The chat used to answer a wall of text with a second, near-identical wall —
//! finding the corrections was the reader's job. The reply bubble now marks them:
//! removed words struck through, added words underlined. Pure functions, no DOM,
//! so the mapping from a pair of texts to a list of edits is testable on its own.

// A token is a word (apostrophes kept inside it), a single punctuation mark, or a
// run of whitespace. Whitespace stands alone so that a missing comma is an edit of
// one character and not of the words on either side of it.
const TOKEN = /[\p{L}\p{N}]+(?:['’][\p{L}\p{N}]+)*|\s+|[^\s\p{L}\p{N}]/gu;

export function tokenize(text) {
  return text.match(TOKEN) ?? [];
}

// The diff is an LCS table — O(n·m) cells over the part that actually differs.
// Past this many tokens on either side the table costs more than the marking is
// worth (a pasted page, not a sentence), so diffWords gives up and the caller
// renders the correction plain.
export const MAX_TOKENS = 1500;

function push(ops, type, text) {
  const last = ops[ops.length - 1];
  if (last && last.type === type) last.text += text;
  else ops.push({ type, text });
}

function lcsOps(a, b) {
  const n = a.length;
  const m = b.length;
  const w = m + 1;
  // dp[i][j] = length of the longest common subsequence of a[i:] and b[j:].
  const dp = new Uint32Array((n + 1) * w);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i * w + j] = a[i] === b[j]
        ? dp[(i + 1) * w + j + 1] + 1
        : Math.max(dp[(i + 1) * w + j], dp[i * w + j + 1]);
    }
  }
  const ops = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      push(ops, "same", a[i]);
      i++;
      j++;
    } else if (dp[(i + 1) * w + j] >= dp[i * w + j + 1]) {
      // Deletions first on a tie, so a replaced word reads "old → new".
      push(ops, "del", a[i++]);
    } else {
      push(ops, "ins", b[j++]);
    }
  }
  while (i < n) push(ops, "del", a[i++]);
  while (j < m) push(ops, "ins", b[j++]);
  return ops;
}

// A word that only changed case ("i" → "I") is the most common correction there
// is, and showing it as a struck-through letter glued to its replacement turns
// every sentence opening into litter. Keep the new word marked, drop the old one.
function dropCaseOnlyDeletions(ops) {
  const out = [];
  for (let i = 0; i < ops.length; i++) {
    const del = ops[i];
    const ins = ops[i + 1];
    if (
      del.type === "del" && ins?.type === "ins" &&
      del.text.toLowerCase() === ins.text.toLowerCase()
    ) {
      out.push(ins);
      i++;
      continue;
    }
    out.push(del);
  }
  return out;
}

// [{ type: "same" | "del" | "ins", text }] in reading order, or null when the
// texts are too long to diff. Concatenating "same"+"ins" gives `after` back;
// "same"+"del" gives `before`, except for words that only changed case.
export function diffWords(before, after) {
  const a = tokenize(before);
  const b = tokenize(after);

  // Corrections touch a few words in a long text, so shave the untouched head and
  // tail before building the table — that is what keeps a paragraph cheap.
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head++;
  let tail = 0;
  while (
    tail < a.length - head &&
    tail < b.length - head &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]
  ) tail++;

  const midA = a.slice(head, a.length - tail);
  const midB = b.slice(head, b.length - tail);
  if (midA.length > MAX_TOKENS || midB.length > MAX_TOKENS) return null;

  const ops = [];
  for (const t of a.slice(0, head)) push(ops, "same", t);
  for (const op of dropCaseOnlyDeletions(lcsOps(midA, midB))) push(ops, op.type, op.text);
  for (const t of a.slice(a.length - tail)) push(ops, "same", t);
  return ops;
}

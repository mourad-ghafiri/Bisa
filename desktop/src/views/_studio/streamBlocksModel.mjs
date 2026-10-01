/**
 * A reply as it streams, cut into the blocks that will not change and the
 * one still being written (13 — Conversations §The reply streams): a blank
 * line outside a fenced block settles everything above it; an open fence
 * settles nothing until it closes; a list or a table continued past a blank
 * line stays one block. A blank line with nothing after it yet settles
 * nothing either: the next line may continue the block, and a settled block
 * is never re-parsed. Each settled block is parsed once and never again — the
 * tail alone is re-parsed as the words arrive, so a long reply costs no more
 * per frame than a short one. `[...settled, tail].join("\n")` is the text
 * again, exactly. Plain `.mjs`, so `node --test` reads it.
 */

const FENCE_OPEN = /^\s{0,3}(`{3,}|~{3,})/;
/** A line that continues the block above it across a blank line: a table row, or an indented list continuation. */
const CONTINUES = /^(\s*\||\s{2,}\S)/;

/**
 * @param {string} text
 * @returns {{ settled: string[], tail: string }}
 */
export function settledBlocks(text) {
  const lines = String(text ?? "").split("\n");
  const settled = [];
  let current = [];
  /** The open fence's marker, or null outside one. */
  let fence = null;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (fence !== null) {
      current.push(line);
      const t = line.trim();
      if (t.startsWith(fence) && /^[`~]+$/.test(t)) fence = null;
      continue;
    }
    const opened = FENCE_OPEN.exec(line);
    if (opened) {
      fence = opened[1];
      current.push(line);
      continue;
    }
    if (line.trim() !== "") {
      current.push(line);
      continue;
    }
    // A blank line: the block above is done — unless what follows continues
    // it, or nothing follows yet and the next line could.
    current.push(line);
    const next = lines.slice(i + 1).find((l) => l.trim() !== "");
    if (next === undefined) continue;
    if (!CONTINUES.test(next) && current.some((l) => l.trim() !== "")) {
      settled.push(current.join("\n"));
      current = [];
    }
  }
  return { settled, tail: current.join("\n") };
}

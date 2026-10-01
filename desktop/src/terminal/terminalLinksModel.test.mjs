/**
 * A link that wraps in a terminal is one door. Run with
 * `node --test desktop/src/terminal/terminalLinksModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { MAX_JOINED_ROWS, linksAt, logicalLine } from "./terminalLinksModel.mjs";

const COLS = 20;
/** A buffer of rows as xterm hands them: full width, padded with spaces; `wrapped` where xterm folded a line. */
const buffer = (rows) => {
  const padded = rows.map((r) => (typeof r === "string" ? { text: r, wrapped: false } : r)).map((r) => ({ ...r, text: r.text.padEnd(COLS, " ") }));
  return (i) => padded[i] ?? null;
};
/** A shell's long line, folded by xterm at `COLS`. */
const softWrap = (line) => {
  const out = [];
  for (let i = 0; i < line.length; i += COLS) out.push({ text: line.slice(i, i + COLS), wrapped: i > 0 });
  return out;
};

test("a path soft-wrapped across two rows is one link with a two-row range and its line", () => {
  const rowAt = buffer(["$ cargo build", ...softWrap("error: src/views/_wb/Center.tsx:301 boom"), ""]);
  const on1 = linksAt(rowAt, 1, COLS);
  assert.equal(on1.length, 1);
  const [link] = on1;
  assert.equal(link.text, "src/views/_wb/Center.tsx:301");
  assert.deepEqual(link.hit, { kind: "path", path: "src/views/_wb/Center.tsx", line: 301, col: null, raw: "src/views/_wb/Center.tsx:301" });
  // "error: " is 7 cells: the link starts at column 8 of row 2 (1-based) and ends on row 3.
  assert.deepEqual(link.range.start, { x: 8, y: 2 });
  assert.equal(link.range.end.y, 3);
  assert.equal(link.range.end.x, "src/views/_wb/Center.tsx:301".length - (COLS - 7), "the last cell of the link on its second row");
  assert.deepEqual(linksAt(rowAt, 2, COLS), on1, "the continuation row answers the same link");
  assert.deepEqual(linksAt(rowAt, 0, COLS), [], "the row before is its own line");
});

test("a URL wrapped the same way is one link, and the same scanner finds it", () => {
  const rowAt = buffer(softWrap("Open https://example.com/a/very/long/sign-in/path to continue"));
  const links = linksAt(rowAt, 0, COLS);
  assert.equal(links.length, 1);
  assert.deepEqual(links[0].hit, { kind: "url", url: "https://example.com/a/very/long/sign-in/path" });
  assert.deepEqual(links[0].range.start, { x: 6, y: 1 });
  assert.equal(links[0].range.end.y, 3);
});

test("a hard-wrapped row — full to its last column, the next starting with a non-space — is joined; a line break someone meant is not", () => {
  // A harness TUI broke the path at the edge with a real newline: no `wrapped` mark.
  const rowAt = buffer(["see crates/bisa-core", "/src/set.rs:12", " next"]);
  const [link] = linksAt(rowAt, 0, COLS);
  assert.equal(link.text, "crates/bisa-core/src/set.rs:12");
  assert.deepEqual(link.hit.path, "crates/bisa-core/src/set.rs");
  assert.deepEqual([link.range.start, link.range.end], [{ x: 5, y: 1 }, { x: 14, y: 2 }]);
  assert.equal(logicalLine(rowAt, 1, COLS).first, 0);
  // The row ends short: a newline the writer meant.
  const short = buffer(["see crates/bisa-cor", "e/src/set.rs", "x"]);
  assert.equal(logicalLine(short, 0, COLS).text.trimEnd(), "see crates/bisa-cor");
  // The row is full but the next starts with a space: two lines.
  const spaced = buffer(["see crates/bisa-core", " /src/set.rs"]);
  assert.equal(logicalLine(spaced, 0, COLS).first, 0);
  assert.equal(logicalLine(spaced, 1, COLS).first, 1);
  // The row is full but ends in a space (a word boundary at the edge): two lines.
  const edgeSpace = buffer(["see crates/bisa-cor ", "e/src/set.rs"]);
  assert.equal(logicalLine(edgeSpace, 1, COLS).first, 1);
});

test("a link on one row keeps a one-row range with 1-based columns, and padding invents nothing", () => {
  const rowAt = buffer(["at src/main.rs:42", "", "https://x.io ok"]);
  const [path] = linksAt(rowAt, 0, COLS);
  assert.deepEqual(path.range, { start: { x: 4, y: 1 }, end: { x: 17, y: 1 } });
  assert.deepEqual(linksAt(rowAt, 1, COLS), []);
  const [url] = linksAt(rowAt, 2, COLS);
  assert.deepEqual(url.range, { start: { x: 1, y: 3 }, end: { x: 12, y: 3 } });
  assert.equal(url.hit.kind, "url");
});

test("a link that begins on a later row of a logical line maps to that row; the walk stops at the cap", () => {
  const rowAt = buffer(softWrap("x".repeat(COLS) + "see src/lib.rs:7"));
  const [link] = linksAt(rowAt, 0, COLS);
  assert.deepEqual(link.range, { start: { x: 5, y: 2 }, end: { x: 16, y: 2 } });
  const long = buffer(softWrap("y".repeat(COLS * (MAX_JOINED_ROWS + 4))));
  const line = logicalLine(long, 0, COLS);
  assert.equal(line.offsets.length, MAX_JOINED_ROWS + 1, "the walk down is bounded");
  const fromEnd = logicalLine(long, MAX_JOINED_ROWS + 3, COLS);
  assert.equal(fromEnd.first, 3, "and so is the walk up");
  assert.deepEqual(logicalLine(buffer([]), 0, COLS), { first: 0, text: "", offsets: [0] }, "no row, no line");
});

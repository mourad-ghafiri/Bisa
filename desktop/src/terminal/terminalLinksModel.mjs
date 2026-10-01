/**
 * The links a terminal row is part of (ide/17 §The terminal), with no
 * xterm in it: a URL or a path a shell or a harness prints is one door even
 * when the row edge cuts it in two. Two ways that happens, both joined here:
 *
 * - **Soft wrap** — a shell prints a long line, xterm folds it and marks the
 *   continuation rows `wrapped`.
 * - **Hard wrap** — a harness TUI draws its own frames row by row and breaks
 *   a long token at the column edge with a real newline: the row before is
 *   full to its last column with a non-space there, and the next row starts
 *   with a non-space. A row that ends short, or a next row that begins with a
 *   space, is a line break someone meant.
 *
 * The **logical line** a row belongs to is the rows joined by those rules,
 * a bounded walk each way; `findLinks` — the app's one scanner for URLs and
 * paths — reads the joined text, and every span is mapped back to the cells
 * it covers, so xterm underlines the whole link across its rows and a click
 * on either row opens the same door.
 *
 * A row's `text` is its full width (`translateToString(false)`): the padding
 * is what says a row ended short, and a wrapped row is exactly `cols` wide.
 */

import { findLinks } from "../ui/linkModel.mjs";

/** How many rows each way a logical line may reach — a runaway join is worse than a cut link. */
export const MAX_JOINED_ROWS = 8;

/** Whether `next` continues `prev` onto its own row. */
function continues(prev, next, cols) {
  if (!prev || !next) return false;
  if (next.wrapped) return true;
  if (prev.text.length < cols) return false;
  return !/\s/.test(prev.text[cols - 1]) && next.text.length > 0 && !/\s/.test(next.text[0]);
}

/**
 * The rows that make one line with row `y`: their joined text, the first
 * row's index, and where each row starts in the text.
 * @param {(i: number) => {text: string, wrapped: boolean} | null} rowAt
 * @param {number} y a buffer row index (0-based)
 * @param {number} cols the terminal's width
 * @returns {{first: number, text: string, offsets: number[]}}
 */
export function logicalLine(rowAt, y, cols) {
  const here = rowAt(y);
  if (!here) return { first: y, text: "", offsets: [0] };
  let first = y;
  for (let up = 0; up < MAX_JOINED_ROWS; up++) {
    const above = rowAt(first - 1);
    if (!continues(above, rowAt(first), cols)) break;
    first -= 1;
  }
  let last = y;
  for (let down = 0; down < MAX_JOINED_ROWS; down++) {
    const below = rowAt(last + 1);
    if (!continues(rowAt(last), below, cols)) break;
    last += 1;
  }
  const offsets = [];
  let text = "";
  for (let i = first; i <= last; i++) {
    offsets.push(text.length);
    text += rowAt(i)?.text ?? "";
  }
  return { first, text, offsets };
}

/** The row (0-based, among the joined rows) holding text offset `at`. */
function rowOf(offsets, at) {
  let row = 0;
  while (row + 1 < offsets.length && offsets[row + 1] <= at) row += 1;
  return row;
}

/**
 * Every link on the logical line row `y` belongs to, each with the cells it
 * covers — xterm's 1-based `{x, y}` range, the end the link's last cell —
 * and the hit the link handler takes.
 * @param {(i: number) => {text: string, wrapped: boolean} | null} rowAt
 * @param {number} y a buffer row index (0-based)
 * @param {number} cols
 * @returns {{range: {start: {x: number, y: number}, end: {x: number, y: number}}, text: string, hit: object}[]}
 */
export function linksAt(rowAt, y, cols) {
  const line = logicalLine(rowAt, y, cols);
  return findLinks(line.text).map((span) => {
    const startRow = rowOf(line.offsets, span.start);
    const endRow = rowOf(line.offsets, span.end - 1);
    return {
      range: {
        start: { x: span.start - line.offsets[startRow] + 1, y: line.first + startRow + 1 },
        end: { x: span.end - line.offsets[endRow], y: line.first + endRow + 1 },
      },
      text: span.raw,
      hit: span.kind === "url" ? { kind: "url", url: span.url } : { kind: "path", path: span.path, line: span.line, col: span.col, raw: span.raw },
    };
  });
}

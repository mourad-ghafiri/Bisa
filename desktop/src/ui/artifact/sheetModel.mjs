/**
 * A spreadsheet as the grid draws it (ide/12): a delimited text parsed by
 * RFC 4180, the delimiter told from the first line, the column widths from
 * the content, and the words a card says about it. Pure; an `.xlsx` is
 * parsed by SheetJS in the viewer and handed here as rows.
 */

import { compileFind } from "../find/findModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** Which of `,`, `;` and a tab the first line uses most. */
export function detectDelimiter(text) {
  const first = String(text ?? "").split(/\r?\n/, 1)[0] ?? "";
  const counts = [",", ";", "\t"].map((d) => [d, first.split(d).length - 1]);
  counts.sort((a, b) => b[1] - a[1]);
  return counts[0][1] > 0 ? counts[0][0] : ",";
}

/**
 * RFC 4180: fields split on the delimiter, a quoted field may hold the
 * delimiter, a newline and a doubled quote; CRLF and LF both end a record;
 * a trailing newline adds no empty record.
 */
export function parseDelimited(text, delimiter = ",") {
  const src = String(text ?? "");
  const rows = [];
  let row = [];
  let field = "";
  let quoted = false;
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (quoted) {
      if (c === '"') {
        if (src[i + 1] === '"') {
          field += '"';
          i += 2;
          continue;
        }
        quoted = false;
        i += 1;
        continue;
      }
      field += c;
      i += 1;
      continue;
    }
    if (c === '"' && field === "") {
      quoted = true;
      i += 1;
      continue;
    }
    if (c === delimiter) {
      row.push(field);
      field = "";
      i += 1;
      continue;
    }
    if (c === "\r" || c === "\n") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
      i += c === "\r" && src[i + 1] === "\n" ? 2 : 1;
      continue;
    }
    field += c;
    i += 1;
  }
  if (field !== "" || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

/** A delimited text as one sheet, its delimiter told from the first line. */
export function sheetFromText(text, name = t("ui-sheet-sheet-1")) {
  return { name, rows: parseDelimited(text, detectDelimiter(text)) };
}

/**
 * The cells a find lands on, in reading order — row by row, left to right,
 * the header row included — each as the grid addresses it. Searched in the
 * model, not the DOM: the grid is virtualised, and a row off screen is not
 * drawn to be searched.
 * @param {readonly unknown[][]} rows
 * @param {{query: string, regex: boolean, caseSensitive: boolean}} find
 * @returns {{row: number, col: number}[]}
 */
export function findCells(rows, find) {
  const re = compileFind(find);
  const out = [];
  if (!re) return out;
  rows.forEach((row, r) => {
    row.forEach((v, c) => {
      re.lastIndex = 0;
      if (re.test(cellText(v))) out.push({ row: r, col: c });
    });
  });
  return out;
}

/** Every row padded to the widest, so the grid has one column count. */
export function squareRows(rows) {
  const width = rows.reduce((w, r) => Math.max(w, r.length), 0);
  return rows.map((r) => (r.length === width ? r : [...r, ...Array(width - r.length).fill("")]));
}

/** A cell as the grid prints it. */
export function cellText(v) {
  if (v === null || v === undefined) return "";
  if (typeof v === "number") return Number.isInteger(v) ? String(v) : String(Number(v.toFixed(6)));
  if (v instanceof Date) return v.toISOString().slice(0, 10);
  return String(v);
}

/**
 * Column widths in characters from the content of the first rows: at least
 * `min`, at most `max`, so a column of ids is narrow and a column of prose
 * does not swallow the grid.
 */
export function columnWidths(rows, { min = 6, max = 48, sample = 200 } = {}) {
  const width = rows.reduce((w, r) => Math.max(w, r.length), 0);
  const out = Array(width).fill(min);
  for (const row of rows.slice(0, sample)) {
    row.forEach((cell, i) => {
      const len = cellText(cell).length;
      if (len > out[i]) out[i] = Math.min(max, len);
    });
  }
  return out;
}

/** The card's fact: *120 rows × 6 columns*, *3 sheets*. */
export function sheetFacts(sheets) {
  if (sheets.length === 0) return "empty";
  if (sheets.length > 1) return t("ui-sheet-sheets", { sheets: sheets.length });
  const rows = sheets[0].rows;
  const cols = rows.reduce((w, r) => Math.max(w, r.length), 0);
  return `${rows.length} ${rows.length === 1 ? "row" : "rows"} × ${cols} ${cols === 1 ? "column" : "columns"}`;
}

/** `A`, `B`, … `Z`, `AA`. */
export function columnLetter(index) {
  let n = index + 1;
  let s = "";
  while (n > 0) {
    const r = (n - 1) % 26;
    s = String.fromCharCode(65 + r) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

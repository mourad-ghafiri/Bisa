/**
 * The sheet grid's facts. Run with `node --test desktop/src/ui/artifact/sheetModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { cellText, columnLetter, columnWidths, detectDelimiter, findCells, parseDelimited, sheetFacts, sheetFromText, squareRows } from "./sheetModel.mjs";

test("a delimited text parses by RFC 4180", () => {
  assert.deepEqual(parseDelimited("a,b,c\n1,2,3\n"), [
    ["a", "b", "c"],
    ["1", "2", "3"],
  ]);
  assert.deepEqual(parseDelimited('name,quote\r\n"Smith, J","said ""hi""\nthen left"\r\n'), [
    ["name", "quote"],
    ["Smith, J", 'said "hi"\nthen left'],
  ]);
  assert.deepEqual(parseDelimited("a;b\n1;2", ";"), [
    ["a", "b"],
    ["1", "2"],
  ]);
  assert.deepEqual(parseDelimited("a,,c\n,,\n"), [
    ["a", "", "c"],
    ["", "", ""],
  ]);
  assert.deepEqual(parseDelimited(""), []);
  assert.deepEqual(parseDelimited("x"), [["x"]]);
});

test("the delimiter is told from the first line", () => {
  assert.equal(detectDelimiter("a,b;c\n"), ",");
  assert.equal(detectDelimiter("a;b;c\n1,2"), ";");
  assert.equal(detectDelimiter("a\tb\tc"), "\t");
  assert.equal(detectDelimiter("single"), ",");
  assert.deepEqual(sheetFromText("a\tb\n1\t2", "T").rows, [
    ["a", "b"],
    ["1", "2"],
  ]);
  assert.equal(sheetFromText("a,b").name, "Sheet 1");
});

test("rows are squared, cells printed, widths bounded, columns lettered", () => {
  assert.deepEqual(squareRows([["a"], ["b", "c"]]), [
    ["a", ""],
    ["b", "c"],
  ]);
  assert.equal(cellText(null), "");
  assert.equal(cellText(3), "3");
  assert.equal(cellText(0.1 + 0.2), "0.3");
  assert.equal(cellText(new Date(Date.UTC(2026, 8, 7))), "2026-09-07");
  assert.equal(cellText(true), "true");
  assert.deepEqual(columnWidths([["id", "a very long cell that goes on and on and on and on and on"], ["1", "x"]]), [6, 48]);
  assert.deepEqual(columnWidths([]), []);
  assert.equal(columnLetter(0), "A");
  assert.equal(columnLetter(25), "Z");
  assert.equal(columnLetter(26), "AA");
  assert.equal(columnLetter(701), "ZZ");
});

test("the card's fact about a sheet", () => {
  assert.equal(sheetFacts([]), "empty");
  assert.equal(sheetFacts([{ name: "a", rows: [["x"]] }]), "1 row × 1 column");
  assert.equal(sheetFacts([{ name: "a", rows: [["x", "y"], ["1", "2"], ["3", "4"]] }]), "3 rows × 2 columns");
  assert.equal(sheetFacts([{ name: "a", rows: [] }, { name: "b", rows: [] }]), "2 sheets");
});

test("a find over the rows lands on cells in reading order, the header included, and reads the model rather than the drawn grid", () => {
  const rows = [
    ["name", "city"],
    ["Ada", "London"],
    ["Alan", "Manchester"],
    [42, "Lonsdale"],
  ];
  assert.deepEqual(findCells(rows, { query: "lon", regex: false, caseSensitive: false }), [
    { row: 1, col: 1 },
    { row: 3, col: 1 },
  ]);
  assert.deepEqual(findCells(rows, { query: "^A", regex: true, caseSensitive: true }), [
    { row: 1, col: 0 },
    { row: 2, col: 0 },
  ]);
  assert.deepEqual(findCells(rows, { query: "42", regex: false, caseSensitive: false }), [{ row: 3, col: 0 }], "a number is searched as the grid prints it");
  assert.deepEqual(findCells(rows, { query: "", regex: false, caseSensitive: false }), []);
  assert.deepEqual(findCells(rows, { query: "[", regex: true, caseSensitive: false }), [], "a refused expression finds nothing");
});

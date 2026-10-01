import { test } from "node:test";
import assert from "node:assert/strict";
import { gutterFor } from "./blameModel.mjs";

const when = (s) => `t${s}`;

test("each row carries the short hash and a padded author, and the whole story on hover", () => {
  const rows = gutterFor(
    [
      { line: 1, commit: "a".repeat(40), short: "aaaaaaa", author: "Ada", timestamp: 5, summary: "first", uncommitted: false },
      { line: 2, commit: "b".repeat(40), short: "bbbbbbb", author: "Grace Hopper", timestamp: 6, summary: "second", uncommitted: false },
    ],
    when,
  );
  assert.equal(rows[0].text, "aaaaaaa Ada         ");
  assert.equal(rows[1].text, "bbbbbbb Grace Hopper");
  assert.equal(rows[0].text.length, rows[1].text.length, "the column lines up");
  assert.match(rows[0].hover, /\*\*aaaaaaa\*\* first/);
  assert.match(rows[0].hover, /Ada · t5/);
});

test("an uncommitted line says so instead of showing zeros, and a long author is cut", () => {
  const rows = gutterFor(
    [
      { line: 1, commit: "0".repeat(40), short: "0000000", author: "Not committed yet", timestamp: 0, summary: "", uncommitted: true },
      { line: 2, commit: "c".repeat(40), short: "ccccccc", author: "Someone With A Very Long Name Indeed", timestamp: 9, summary: "x", uncommitted: false },
    ],
    when,
  );
  assert.ok(!rows[0].text.includes("0000000"));
  assert.match(rows[0].hover, /Not committed yet/);
  assert.equal(rows[1].text.length, rows[0].text.length);
  assert.ok(rows[1].text.endsWith("…"), rows[1].text);
});

test("no rows is no gutter", () => {
  assert.deepEqual(gutterFor([], when), []);
});

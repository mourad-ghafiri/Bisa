/**
 * The Files tab's search as facts. Run with `node --test desktop/src/views/_workbench/fileSearchModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { groupHits, nameResults, nextOpenable, parseQuery, resultRows, searchStatus, splitHit } from "./fileSearchModel.mjs";

test("the box parses into a needle and the globs that narrow where", () => {
  assert.deepEqual(parseQuery("  todo  in:src/** -in:*.lock fixme "), { needle: "todo fixme", include: ["src/**"], exclude: ["*.lock"] });
  assert.deepEqual(parseQuery(""), { needle: "", include: [], exclude: [] });
  assert.deepEqual(parseQuery("in: -in:"), { needle: "in: -in:", include: [], exclude: [] }, "a bare token is words, not an empty glob");
});

test("names are ranked by the palette's scorer, so the same query finds the same files here", () => {
  const index = ["src/cart/total.ts", "docs/cart.md", "src/lib.rs", "README.md"];
  const hits = nameResults(index, "cart");
  assert.deepEqual([...hits].sort(), ["docs/cart.md", "src/cart/total.ts"]);
  assert.deepEqual(nameResults(index, "  "), [], "nothing typed finds nothing — the tree is right there");
  assert.equal(nameResults(index, "src", 1).length, 1, "the cap holds");
});

test("hits fold into their files in arrival order, keeping the first few and counting the rest", () => {
  const hit = (path, line) => ({ path, line, column: 1, text: `line ${line}` });
  const hits = [hit("b.rs", 3), hit("a.rs", 1), hit("b.rs", 9), hit("b.rs", 12), hit("a.rs", 2)];
  const groups = groupHits(hits, 2);
  assert.deepEqual(groups.map((g) => g.path), ["b.rs", "a.rs"], "the order files first appeared, not alphabetical");
  assert.deepEqual(groups[0].hits.map((h) => h.line), [3, 9]);
  assert.equal(groups[0].more, 1, "the third hit in b.rs is counted, not shown");
  assert.equal(groups[1].more, 0);
  const rows = resultRows(groups);
  assert.deepEqual(rows.map((r) => r.kind), ["file", "hit", "hit", "more", "file", "hit", "hit"]);
  assert.equal(rows[0].count, 3, "the file row counts every hit, shown or folded");
  assert.equal(rows[3].more, 1);
  assert.equal(nextOpenable(rows, -1, 1), 0);
  assert.equal(nextOpenable(rows, 2, 1), 4, "Enter skips the '+N more' note");
  assert.equal(nextOpenable(rows, 4, -1), 2, "and backwards");
  assert.equal(nextOpenable(rows, 6, 1), 0, "wraps");
  assert.equal(nextOpenable([], 0, 1), -1);
  assert.equal(nextOpenable([{ kind: "more", key: "m", path: "x", more: 1 }], -1, 1), -1, "a note alone is nothing to open");
});

test("the footer says how many, where, and whether the cap stopped it", () => {
  assert.equal(searchStatus(null, true, 0), "searching…");
  assert.equal(searchStatus(null, true, 12), "12 so far…");
  assert.equal(searchStatus(null, false, 0), "");
  assert.equal(searchStatus({ matches: 0, files_with_matches: 0, files_scanned: 300, truncated: false }, false, 0), "no match in 300 files");
  assert.equal(searchStatus({ matches: 1, files_with_matches: 1, files_scanned: 300, truncated: false }, false, 1), "1 match in 1 file · 300 scanned");
  assert.equal(searchStatus({ matches: 12, files_with_matches: 4, files_scanned: 1203, truncated: false }, false, 12), "12 matches in 4 files · 1203 scanned");
  assert.match(searchStatus({ matches: 2000, files_with_matches: 90, files_scanned: 5000, truncated: true }, false, 2000), /stopped at the cap; narrow it/);
});

test("a hit's line splits around the match so the row can mark it", () => {
  assert.deepEqual(splitHit("let cart = total();", 5, 4), ["let ", "cart", " = total();"]);
  assert.deepEqual(splitHit("cart", 1, 4), ["", "cart", ""]);
  assert.deepEqual(splitHit("x", 0, 0), ["", "", "x"], "a column the node did not give leaves the line whole");
});

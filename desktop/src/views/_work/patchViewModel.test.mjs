/**
 * The views of a changed file's patch. Run with `node --test desktop/src/views/_work/patchViewModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PATCH_VIEWS, PATCH_VIEW_LABEL, needsSides, patchViewGlyph, patchViewHint, sidesWords } from "./patchViewModel.mjs";

test("three views, the hunks first, each with a word, a glyph the kit draws and a tooltip that says read-only where it is", () => {
  assert.deepEqual([...PATCH_VIEWS], ["hunks", "split", "inline"]);
  for (const v of PATCH_VIEWS) assert.ok(PATCH_VIEW_LABEL[v], `${v} has a word`);
  assert.equal(PATCH_VIEW_LABEL.split, "Side by side");
  const icons = readFileSync(new URL("../../ui/icons.ts", import.meta.url), "utf8");
  for (const v of PATCH_VIEWS) assert.match(icons, new RegExp(`^  ${patchViewGlyph(v)}: `, "m"), `${v} wears a glyph that exists`);
  assert.equal(new Set(PATCH_VIEWS.map(patchViewGlyph)).size, 3, "each view its own glyph");
  assert.match(patchViewHint("hunks"), /stage, pick lines, annotate/, "the acts live on the hunks");
  assert.match(patchViewHint("hunks", { readOnly: true }), /read-only/, "a commit's hunks are history: no acts promised");
  assert.doesNotMatch(patchViewHint("hunks", { readOnly: true }), /stage/);
  assert.match(patchViewHint("split"), /read-only/);
  assert.match(patchViewHint("inline"), /read-only/);
});

test("a comparison needs the two whole texts; the hunks need the patch", () => {
  assert.ok(needsSides("split") && needsSides("inline"));
  assert.ok(!needsSides("hunks"));
});

test("the sentence over a comparison names a binary file, an empty side and a cut — or is nothing", () => {
  const sides = (over = {}) => ({ original: "a\n", modified: "b\n", binary: false, truncated: false, ...over });
  assert.equal(sidesWords(sides()), null, "two texts speak for themselves");
  assert.equal(sidesWords(null), null);
  assert.equal(sidesWords(sides({ binary: true })), "Binary file — no text to compare.");
  assert.equal(sidesWords(sides({ original: null })), "New — the left side is empty.");
  assert.equal(sidesWords(sides({ modified: null })), "Deleted — the right side is empty.");
  assert.equal(sidesWords(sides({ truncated: true })), "The first 2 MiB of each side are shown.");
  assert.equal(sidesWords(sides({ original: null, truncated: true })), "New — the left side is empty. The first 2 MiB of each side are shown.");
  assert.equal(sidesWords(sides({ original: null, modified: null })), null, "nothing on either side says nothing");
});

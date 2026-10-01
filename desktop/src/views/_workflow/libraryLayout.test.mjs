/**
 * The library's layout, held where it is decided: one grid, shared, filling
 * the screen like every other list. A source guard, as `deadExports.test.mjs`
 * is — there is no DOM here to measure.
 *
 * Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { LIBRARY_GRID } from "./libraryLayout.mjs";

const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");

test("the library's cards fill the screen: one grid the library and the gallery share, columns added as the window widens, no centred column", () => {
  assert.match(LIBRARY_GRID, /^grid /);
  assert.ok(!/max-w-|mx-auto/.test(LIBRARY_GRID), "the grid caps nothing");
  assert.deepEqual(LIBRARY_GRID.match(/[a-z0-9]+:grid-cols-\d/g), ["sm:grid-cols-2", "lg:grid-cols-3", "xl:grid-cols-4"], "two, three, then four across");
  const screen = read("../Workflows.tsx");
  const gallery = read("./TemplateGallery.tsx");
  assert.ok(screen.includes("className={LIBRARY_GRID}") && gallery.includes("className={LIBRARY_GRID}"), "both draw the one grid");
  assert.ok(!screen.includes("max-w-5xl") && !/className="[^"]*\bmx-auto max-w-/.test(screen), "the list is the body's width, like the Goals list");
  assert.ok(!/grid-cols-\d/.test(screen) && !/grid-cols-\d/.test(gallery), "no second grid written by hand");
  assert.ok(screen.includes('className="min-h-0 flex-1 overflow-y-auto px-6 py-4"'), "the same body the Goals list has");
});

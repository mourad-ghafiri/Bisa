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

test("the library's cards fill the screen: one grid the library and the gallery share, columns added as the room widens, no centred column", () => {
  assert.match(LIBRARY_GRID, /^grid /);
  assert.ok(!/max-w-|mx-auto/.test(LIBRARY_GRID), "the grid caps nothing");
  assert.ok(LIBRARY_GRID.includes("grid-cols-[repeat(auto-fill,minmax(13.75rem,1fr))]"), "as many 13.75rem columns as the list's room holds");
  // The body beside a 280px sidebar, less its 24px gutters and the grid's 12px gaps: 3, 4 and 6 across.
  const across = (window) => Math.floor((window - 280 - 48 + 12) / (13.75 * 16 + 12));
  assert.deepEqual([1024, 1440, 1920].map(across), [3, 4, 6]);
  assert.ok(!/(sm|md|lg|xl|2xl):grid-cols-/.test(LIBRARY_GRID), "read from the room, never the window");
  const screen = read("../Workflows.tsx");
  const gallery = read("./TemplateGallery.tsx");
  assert.ok(screen.includes("className={LIBRARY_GRID}") && gallery.includes("className={LIBRARY_GRID}"), "both draw the one grid");
  assert.ok(!screen.includes("max-w-5xl") && !/className="[^"]*\bmx-auto max-w-/.test(screen), "the list is the body's width, like the Goals list");
  assert.ok(!/grid-cols-\d/.test(screen) && !/grid-cols-\d/.test(gallery), "no second grid written by hand");
  assert.ok(screen.includes('className="min-h-0 flex-1 overflow-y-auto px-6 py-4"'), "the same body the Goals list has");
});

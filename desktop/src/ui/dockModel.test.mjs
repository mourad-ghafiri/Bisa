/**
 * Where a floating dock sits, tested where it lives.
 *
 * What can actually be wrong in a way a person notices: a dock that is not
 * where they left it after the window changed size, one that jumps when it is
 * grabbed, one parked somewhere the pointer cannot reach it, or one painted
 * over the chrome so a drag moves the OS window instead.
 *
 * Run with `node --test desktop/src/ui/dockModel.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { dockBox, placementFrom, placementOf, samePlacement } from "./dockModel.mjs";

/** The window as it opens, maximized, and shrunk. */
const A = { width: 1240, height: 820, size: 48, top: 40, margin: 8 };
const B = { ...A, width: 2560, height: 1400 };
const C = { ...A, width: 600, height: 500 };

test("the defaults paint where they say: 24 from the right and 24 from the bottom", () => {
  // Not 48 from the bottom: the chrome keep-out belongs to the top edge.
  assert.deepEqual(dockBox({ h: "right", x: 24, v: "bottom", y: 24 }, A), { left: 1168, top: 748 });
  assert.deepEqual(dockBox({ h: "left", x: 16, v: "top", y: 40 }, A), { left: 16, top: 40 });
});

test("a dock parked top-left stays top-left when the window is maximized", () => {
  const p = { h: "left", x: 16, v: "top", y: 40 };
  assert.deepEqual(dockBox(p, A), dockBox(p, B), "it keeps its distance from the edges it lives near");
  const q = { h: "right", x: 24, v: "bottom", y: 24 };
  assert.deepEqual(dockBox(q, B), { left: 2488, top: 1328 }, "a corner dock is still 24 from its corner");
});

test("a maximize and a restore put the dock back exactly, and a shrink clamps the paint without touching the placement", () => {
  // Placed in the window as it opens: 560 from the left, 24 from the bottom.
  const p = placementOf({ left: 560, top: 748 }, A);
  assert.deepEqual(p, { h: "left", x: 560, v: "bottom", y: 24 });
  assert.deepEqual(placementOf(dockBox(p, B), B), p, "maximized: the paint reads back as the same placement");
  assert.deepEqual(placementOf(dockBox(p, A), A), p, "restored: the same");
  const shrunk = dockBox(p, C);
  assert.equal(shrunk.left, 544, "560 from the left of a 600px window is pulled inside it");
  assert.deepEqual(p, { h: "left", x: 560, v: "bottom", y: 24 }, "the placement is the fact; the clamp is not written to it");
  assert.deepEqual(dockBox(p, A), { left: 560, top: 748 }, "grown again, it is back where it was");
});

test("a box is anchored to the nearer edge on each axis, and a tie goes to the right and the bottom", () => {
  assert.deepEqual(placementOf({ left: 100, top: 100 }, A), { h: "left", x: 100, v: "top", y: 100 });
  assert.deepEqual(placementOf({ left: 1100, top: 700 }, A), { h: "right", x: 92, v: "bottom", y: 72 });
  assert.deepEqual(placementOf({ left: 100, top: 700 }, A), { h: "left", x: 100, v: "bottom", y: 72 }, "each axis on its own");
  // The travel is [8, 1184]; its midpoint 596 is as far from either edge.
  assert.equal(placementOf({ left: 596, top: 100 }, A).h, "right");
  assert.equal(placementOf({ left: 596, top: 100 }, A).x, 596);
});

test("the keep-out is the chrome at the top, and the bottom margin is reachable", () => {
  assert.equal(dockBox({ h: "left", x: 500, v: "top", y: 0 }, A).top, 40, "never over the drag region");
  assert.equal(placementOf({ left: 500, top: 0 }, A).y, 40);
  assert.equal(dockBox({ h: "right", x: 24, v: "bottom", y: 0 }, A).top, 764, "8 from the bottom");
  assert.equal(placementOf({ left: 500, top: 5000 }, A).y, 8);
  assert.equal(dockBox({ h: "left", x: 500, v: "top", y: 0 }, { ...A, top: 36 }).top, 36, "the compact chrome is shorter, and the model takes the theme's word for it");
});

test("a drag begins from the painted box, so a clamped dock does not jump when grabbed", () => {
  const p = { h: "right", x: 24, v: "bottom", y: 600 };
  const box = dockBox(p, C);
  assert.deepEqual(dockBox(placementOf(box, C), C), box, "a zero-travel move paints exactly where it was");
  const moved = placementOf({ left: box.left + 10, top: box.top + 10 }, C);
  assert.deepEqual(dockBox(moved, C), { left: box.left + 10, top: box.top + 10 }, "moved by exactly the pointer's travel");
  assert.deepEqual(moved, { h: "right", x: 14, v: "top", y: 50 });
});

test("the dock is never stranded: a wild box lands inside, and a degenerate window still yields a reachable point", () => {
  const p = placementOf({ left: -100, top: 5000 }, A);
  const box = dockBox(p, A);
  assert.ok(box.left >= 8 && box.left <= 1184 && box.top >= 40 && box.top <= 764, `${box.left},${box.top} is off the window`);
  const tiny = { width: 40, height: 40, top: 40 };
  for (const q of [dockBox(p, tiny), placementOf({ left: 10, top: 10 }, tiny)]) {
    for (const n of Object.values(q)) if (typeof n === "number") assert.ok(Number.isFinite(n) && n >= 0);
  }
});

test("a dock taller than it is wide is clamped by its own height; size is the square fallback", () => {
  // The pet is 96×104: clamped as a 40px square its bottom hung off the screen.
  const pet = { width: 1000, height: 800, sizeX: 96, sizeY: 104, top: 40 };
  assert.deepEqual(dockBox({ h: "right", x: 96, v: "bottom", y: 24 }, pet), { left: 808, top: 672 });
  assert.deepEqual(dockBox({ h: "right", x: 0, v: "bottom", y: 0 }, pet), { left: 896, top: 688 });
  assert.deepEqual(dockBox({ h: "right", x: 0, v: "bottom", y: 0 }, { ...pet, sizeX: 104, sizeY: 96 }), { left: 888, top: 696 }, "each axis uses its own extent");
  const square = { width: 1000, height: 800, size: 40, top: 40 };
  assert.deepEqual(dockBox({ h: "right", x: 0, v: "bottom", y: 0 }, square), dockBox({ h: "right", x: 0, v: "bottom", y: 0 }, { ...square, sizeX: 40, sizeY: 40 }));
});

test("a stored value is a placement only when it is exactly one; anything else takes the fallback", () => {
  const ok = { h: "left", x: 16, v: "top", y: 40 };
  const read = placementFrom(ok, null);
  assert.deepEqual(read, ok);
  assert.notEqual(read, ok, "a fresh object, never the stored reference");
  const FALLBACK = { h: "right", x: 24, v: "bottom", y: 24 };
  for (const bad of [{ x: 24, y: 24 }, null, undefined, "x", 3, { h: "middle", x: 1, v: "top", y: 1 }, { h: "left", x: NaN, v: "top", y: 1 }, { h: "left", x: -1, v: "top", y: 1 }, { h: "left", x: 1, v: "top" }, { h: "left", x: "1", v: "top", y: 1 }]) {
    assert.equal(placementFrom(bad, FALLBACK), FALLBACK, `${JSON.stringify(bad)} is not a placement`);
    assert.equal(placementFrom(bad, null), null);
  }
});

test("two placements are the same only from the same edges", () => {
  assert.equal(samePlacement({ h: "right", x: 24, v: "bottom", y: 24 }, { h: "right", x: 24, v: "bottom", y: 24 }), true);
  assert.equal(samePlacement({ h: "right", x: 24, v: "bottom", y: 24 }, { h: "left", x: 24, v: "bottom", y: 24 }), false);
  assert.equal(samePlacement({ h: "right", x: 24, v: "bottom", y: 24 }, { h: "right", x: 24, v: "bottom", y: 25 }), false);
});

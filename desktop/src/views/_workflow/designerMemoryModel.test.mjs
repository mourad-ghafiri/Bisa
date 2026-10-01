/**
 * What a canvas keeps of where it was left: the step it had picked and
 * where it looked, made safe to open on. Run with
 * `node --test desktop/src/views/_workflow/designerMemoryModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { NOTHING, keptSelection, parseMemory, sameViewport, keptStep, viewportOf } from "./designerMemoryModel.mjs";

test("a canvas never opened remembers nothing, and opens fitted", () => {
  assert.deepEqual(parseMemory({ selected: undefined, viewport: undefined }), { selected: null, viewport: null });
  assert.equal(parseMemory({}), NOTHING);
  assert.equal(parseMemory(null), NOTHING);
  assert.equal(parseMemory(undefined), NOTHING);
  assert.ok(Object.isFrozen(NOTHING));
});

test("the step picked and the place looked at read back as they were kept, each on its own", () => {
  const kept = JSON.parse(JSON.stringify({ selected: "build", viewport: { x: -120, y: 40, zoom: 1.5 } }));
  assert.deepEqual(parseMemory(kept), { selected: "build", viewport: { x: -120, y: 40, zoom: 1.5 } });
  assert.deepEqual(parseMemory({ selected: "review" }), { selected: "review", viewport: null }, "a pick with no place opens fitted, on the step");
  assert.deepEqual(parseMemory({ viewport: { x: 0, y: 0, zoom: 0.5 } }), { selected: null, viewport: { x: 0, y: 0, zoom: 0.5 } }, "a place with no pick");
});

test("what the memory gives back that is no step or no place is nothing, and spoils nothing beside it", () => {
  assert.deepEqual(parseMemory({ selected: 7, viewport: { x: 1, y: 2, zoom: 1 } }), { selected: null, viewport: { x: 1, y: 2, zoom: 1 } });
  assert.deepEqual(parseMemory({ selected: "build", viewport: "here" }), { selected: "build", viewport: null });
  for (const bad of [null, undefined, "", 7, true, {}, ["build"], "s".repeat(513)]) assert.equal(keptStep(bad), null, JSON.stringify(bad));
  assert.equal(keptStep("build"), "build");
});

test("a viewport that is no place is not opened on", () => {
  for (const bad of [null, undefined, "here", { x: 1, y: 2 }, { x: Number.NaN, y: 0, zoom: 1 }, { x: 0, y: Infinity, zoom: 1 }, { x: 0, y: 0, zoom: 0 }, { x: 0, y: 0, zoom: -1 }, { x: "1", y: 0, zoom: 1 }]) {
    assert.equal(viewportOf(bad), null, JSON.stringify(bad));
  }
  assert.deepEqual(viewportOf({ x: 3, y: -4, zoom: 0.25, extra: true }), { x: 3, y: -4, zoom: 0.25 }, "only the pan and the zoom are kept");
});

test("two viewports are the same place when the pan and the zoom are", () => {
  assert.ok(sameViewport({ x: 1, y: 2, zoom: 1 }, { x: 1, y: 2, zoom: 1 }));
  assert.ok(!sameViewport({ x: 1, y: 2, zoom: 1 }, { x: 1, y: 2, zoom: 2 }));
  assert.ok(!sameViewport(null, { x: 1, y: 2, zoom: 1 }));
  assert.ok(!sameViewport(null, null), "no place is the same as no other");
});

test("a remembered step the workflow no longer has is no pick", () => {
  const steps = [{ id: "build" }, { id: "ship" }];
  assert.equal(keptSelection("build", steps), "build");
  assert.equal(keptSelection("gone", steps), null, "an agent's save took it");
  assert.equal(keptSelection(null, steps), null);
  assert.equal(keptSelection("build", null), null);
});

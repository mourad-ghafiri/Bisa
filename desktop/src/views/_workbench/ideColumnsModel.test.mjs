/**
 * How the Project IDE's columns share a window. Run with
 * `node --test desktop/src/views/_workbench/ideColumnsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { CENTRE_MIN, fitColumns } from "./ideColumnsModel.mjs";

const at = (total, over = {}) => fitColumns({ total, fixed: 40, rail: 380, railMin: 220, railOpen: true, right: 380, rightMin: 300, rightOpen: true, ...over });
const centre = (total, fit) => total - 40 - (fit.rail ?? 0) - (fit.right ?? 0);

test("with room, every column is as the person set it", () => {
  assert.deepEqual(at(1440 - 280), { rail: 380, right: 380, railFolded: false });
  assert.deepEqual(at(0), { rail: 380, right: 380, railFolded: false }, "before the row is measured: as set");
});

test("short of room, the right panel gives way first, then the rail, each to its least", () => {
  const a = at(1100);
  assert.equal(a.rail, 380, "the rail keeps its width while the right panel can give");
  assert.equal(centre(1100, a), CENTRE_MIN);
  const b = at(960);
  assert.equal(b.right, 300, "the right panel at its least");
  assert.ok(b.rail < 380 && b.rail >= 220, "then the rail gives");
  assert.equal(centre(960, b), CENTRE_MIN);
});

test("with no room for three columns the rail folds — the centre never goes", () => {
  const fit = at(1024 - 280);
  assert.deepEqual(fit, { rail: null, right: 300, railFolded: true });
  assert.ok(centre(744, fit) >= CENTRE_MIN, "the centre keeps its least");
  assert.deepEqual(at(744, { rightOpen: false }), { rail: 344, right: null, railFolded: false }, "with the right panel shut the rail has the room, giving what the centre needs");
  assert.deepEqual(at(744, { railOpen: false }), { rail: null, right: 344, railFolded: false }, "a rail the person shut is not folded, only shut");
});

test("a width the person set below the least is kept, never raised", () => {
  const fit = at(1100, { right: 280 });
  assert.equal(fit.right, 280);
});

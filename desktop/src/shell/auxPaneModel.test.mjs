/**
 * The Details pane's toggle and its width. Run with `node --test desktop/src/shell/auxPaneModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { AUX_DEFAULT_WIDTH, AUX_MAX_SHARE, AUX_MIN_WIDTH, AUX_WIDTH_KEY, auxBounds, shownWidth, toggledAux } from "./auxPaneModel.mjs";

test("the same occupant closes the pane — with no tab in mind, or with the tab it shows; another tab switches; another occupant opens", () => {
  assert.equal(toggledAux({ kind: "browser", id: "b3" }, "browser"), null, "⌘⇧L on a pane opened on a tab closes it at once");
  assert.equal(toggledAux({ kind: "browser", id: null }, "browser"), null);
  assert.equal(toggledAux({ kind: "browser", id: "b3" }, "browser", "b3"), null, "the tab it already shows");
  assert.deepEqual(toggledAux({ kind: "browser", id: "b3" }, "browser", "b4"), { aux: "browser", auxId: "b4" }, "another tab of the same occupant switches");
  assert.deepEqual(toggledAux({ kind: "inspector", id: null }, "browser"), { aux: "browser", auxId: null });
  assert.deepEqual(toggledAux({ kind: null, id: null }, "browser", "b1"), { aux: "browser", auxId: "b1" }, "a closed pane opens on the tab");
});

test("the pane may take seven tenths of the room, never less than its floor", () => {
  assert.equal(AUX_MAX_SHARE, 0.7);
  assert.deepEqual(auxBounds(1000), { min: 300, max: 700 });
  assert.deepEqual(auxBounds(1440), { min: 300, max: 1008 });
  assert.deepEqual(auxBounds(1001), { min: 300, max: 700 }, "whole pixels");
  assert.deepEqual(auxBounds(400), { min: 300, max: 300 }, "a narrow column: the floor twice, so the handle keeps a range");
  assert.equal(AUX_MIN_WIDTH, 300);
  assert.equal(AUX_DEFAULT_WIDTH, 380);
  assert.equal(AUX_WIDTH_KEY, "bisa.aux.width");
});

test("a column not yet measured bounds nothing above the floor", () => {
  for (const unmeasured of [0, -1, null, undefined, Number.NaN, "wide"]) {
    const b = auxBounds(unmeasured);
    assert.equal(b.min, AUX_MIN_WIDTH);
    assert.equal(b.max, Number.POSITIVE_INFINITY, String(unmeasured));
    assert.equal(shownWidth(900, b), 900, "the stored width opens as chosen");
  }
});

test("what is drawn is the chosen width held within the room, the choice itself untouched", () => {
  const b = auxBounds(1000);
  assert.equal(shownWidth(500, b), 500);
  assert.equal(shownWidth(900, b), 700, "past the room: drawn at the bound");
  assert.equal(shownWidth(120, b), 300, "under the floor: drawn at the floor");
  assert.equal(shownWidth(900, auxBounds(2000)), 900, "the room grown back: the choice returns");
});

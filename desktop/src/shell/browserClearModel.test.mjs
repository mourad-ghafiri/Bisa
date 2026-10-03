import { test } from "node:test";
import assert from "node:assert/strict";
import { MAX_CLEARS, clearOf, clearsOver, meets, sameClear, sameClears } from "./browserClearModel.mjs";

test("a box is cut in whole pixels rounded outward, its corner no rounder than half its short side", () => {
  assert.deepEqual(clearOf({ left: 10.4, top: 20.6, width: 100.2, height: 50 }, 16), { left: 10, top: 20, width: 101, height: 51, radius: 16 });
  assert.equal(clearOf({ left: 0, top: 0, width: 48, height: 48 }, 9999).radius, 24, "a round dock: fully round");
  assert.equal(clearOf({ left: 0, top: 0, width: 48, height: 48 }, -1).radius, 0);
  assert.equal(clearOf({ left: 0, top: 0, width: 48, height: 48 }, Number.NaN).radius, 0);
});

test("an empty or broken box cuts nothing", () => {
  assert.equal(clearOf({ left: 0, top: 0, width: 0, height: 10 }, 0), null);
  assert.equal(clearOf({ left: Number.NaN, top: 0, width: 10, height: 10 }, 0), null);
  assert.equal(clearOf({ left: 0, top: 0, width: 10, height: Number.POSITIVE_INFINITY }, 0), null);
});

test("boxes meet when they share area; touching edges do not", () => {
  const a = { left: 0, top: 0, width: 10, height: 10 };
  assert.equal(meets(a, { left: 5, top: 5, width: 10, height: 10 }), true);
  assert.equal(meets(a, { left: 10, top: 0, width: 10, height: 10 }), false);
});

const centre = { visible: true, layer: "browser", rect: { left: 300, top: 40, width: 800, height: 700 } };
const panel = { id: "notes-panel", clear: { left: 900, top: 200, width: 560, height: 500, radius: 16 } };
const dock = { id: "notes-dock", clear: { left: 1360, top: 820, width: 48, height: 48, radius: 24 } };

test("only the overlays over a browser tab's slot are cut — whole, in a steady order", () => {
  assert.deepEqual(clearsOver([panel, dock], [centre]), [panel.clear], "the dock beside the page costs nothing");
  assert.deepEqual(clearsOver([dock, panel], [centre, { visible: true, layer: "browser", rect: { left: 1200, top: 600, width: 300, height: 300 } }]), [dock.clear, panel.clear], "sorted by id, the same frame to frame");
  assert.deepEqual(clearsOver([panel], [{ ...centre, visible: false }]), [panel.clear], "a tab hidden for a dialog keeps its holes: it shows again already cut, never over the panel for a frame");
  assert.deepEqual(clearsOver([panel], [{ ...centre, layer: "terminal" }]), [], "a terminal is not a page");
  assert.deepEqual(clearsOver([panel], [{ ...centre, rect: null }]), []);
  assert.deepEqual(clearsOver([{ id: "gone", clear: null }], [centre]), []);
});

test("the count is the shell's cap", () => {
  const many = Array.from({ length: 30 }, (_, i) => ({ id: `a${String(i).padStart(2, "0")}`, clear: { left: 400 + i, top: 100, width: 10, height: 10, radius: 0 } }));
  assert.equal(clearsOver(many, [centre]).length, MAX_CLEARS);
});

test("an unchanged list is the same list", () => {
  assert.equal(sameClears([panel.clear], [{ ...panel.clear }]), true);
  assert.equal(sameClears([panel.clear], [{ ...panel.clear, top: 201 }]), false);
  assert.equal(sameClears([], [panel.clear]), false);
  assert.equal(sameClear(null, null), true);
  assert.equal(sameClear(panel.clear, null), false);
});

/**
 * Where a tab's webview is drawn. Run with `node --test desktop/src/shell/browserPlacementModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { HOSTS, OFFSTAGE_VIEWPORT, PLACEMENT_TOLERANCE, isShown, offstageRect, placedAsAsked, placementOf, shownTab } from "./browserPlacementModel.mjs";

const box = { left: 10, top: 20, width: 800, height: 600 };
const pane = { left: 900, top: 20, width: 380, height: 600 };
const slot = (key, rect = box, layer = "browser") => ({ rect, layer, key });

test("a tab draws over the host that stands for it, the centre before the pane, and nowhere while a surface is open", () => {
  assert.deepEqual([...HOSTS], ["center", "aux"]);
  assert.deepEqual(placementOf("b1", { center: slot("b1"), aux: null, mayShow: true }), { host: "center", rect: box });
  assert.deepEqual(placementOf("b1", { center: null, aux: slot("b1", pane), mayShow: true }), { host: "aux", rect: pane });
  assert.deepEqual(placementOf("b1", { center: slot("b1"), aux: slot("b1", pane), mayShow: true }), { host: "center", rect: box }, "in both: the bigger box");
  assert.deepEqual(placementOf("b2", { center: slot("b1"), aux: slot("b2", pane), mayShow: true }), { host: "aux", rect: pane }, "each tab its own host");
  assert.equal(placementOf("b3", { center: slot("b1"), aux: slot("b2", pane), mayShow: true }), null);
  assert.equal(placementOf("b1", { center: slot("b1"), aux: null, mayShow: false }), null, "a dialog is open: the layer hides");
  assert.equal(placementOf("b1", { center: slot("b1", box, "terminal"), aux: null, mayShow: true }), null, "the centre shows a terminal");
  assert.equal(placementOf("b1", { center: slot("b1", null), aux: null, mayShow: true }), null, "no rect yet");
  assert.equal(placementOf("b1", { center: slot("b1", { ...box, height: 0 }), aux: null, mayShow: true }), null, "a collapsed box shows nothing");
  assert.ok(isShown("b1", { center: slot("b1"), aux: null, mayShow: true }));
  assert.ok(!isShown("b1", { center: null, aux: null, mayShow: true }));
});


test("the tab a person looks at is the centre's, else the pane's, else none", () => {
  assert.equal(shownTab({ center: slot("b1"), aux: slot("b2", pane) }), "b1");
  assert.equal(shownTab({ center: null, aux: slot("b2", pane) }), "b2");
  assert.equal(shownTab({ center: slot("b1", box, "terminal"), aux: null }), null, "a terminal in the centre is not a browser tab");
  assert.equal(shownTab({ center: null, aux: null }), null);
});

test("a headless tab draws offstage — wholly outside the view, a page's viewport wide, a dialog changing nothing — and counts as shown; a host that claims it still wins", () => {
  const off = offstageRect();
  assert.ok(off.left + off.width < 0 && off.top + off.height < 0, "never a pixel on screen");
  assert.deepEqual([off.width, off.height], [OFFSTAGE_VIEWPORT.width, OFFSTAGE_VIEWPORT.height]);
  assert.deepEqual(placementOf("b1", { center: null, aux: null, mayShow: true, headless: true }), { host: "offstage", rect: off });
  assert.deepEqual(placementOf("b1", { center: null, aux: null, mayShow: false, headless: true }), { host: "offstage", rect: off }, "a dialog is open: offstage is offstage");
  assert.deepEqual(placementOf("b1", { center: slot("b1"), aux: null, mayShow: true, headless: true }), { host: "center", rect: box }, "a host that stands for it — the person revealed it — wins");
  assert.equal(placementOf("b1", { center: null, aux: null, mayShow: true, headless: false }), null, "a seen tab nobody shows is hidden");
  assert.ok(isShown("b1", { center: null, aux: null, mayShow: true, headless: true }), "a screenshot of a headless tab is taken where it renders");
});

test("a placement is as asked when the box read back sits within a pixel of it and shows; a hidden or strayed box is not", () => {
  const asked = { left: 240, top: 96, width: 800, height: 540 };
  assert.ok(placedAsAsked(asked, { ...asked, shown: true }));
  assert.ok(placedAsAsked(asked, { left: 240.5, top: 96, width: 799.5, height: 540, shown: true }), "a rounding is within the tolerance");
  assert.ok(!placedAsAsked(asked, { ...asked, top: 96 + PLACEMENT_TOLERANCE + 1, shown: true }), "a box drawn lower than asked");
  assert.ok(!placedAsAsked(asked, { left: 0, top: 0, width: 1, height: 1, shown: true }), "the box it was opened at, never moved");
  assert.ok(!placedAsAsked(asked, { ...asked, shown: false }), "hidden is not placed");
  assert.ok(!placedAsAsked(asked, null));
});

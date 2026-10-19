import test from "node:test";
import assert from "node:assert/strict";
import { frameTaken, modeAfter, obstaclesOf, pointInSolid, sceneBox, sceneEvent, sceneKey, solidClears } from "./addonSceneModel.mjs";

const vp = { width: 1024, height: 700, top: 40 };

test("the scene is everything under the chrome, and takes the pointer in hunt or over a solid region unshielded", () => {
  assert.deepEqual(sceneBox(vp), { left: 0, top: 40, width: 1024, height: 660 });
  assert.deepEqual(sceneBox({ width: 0, height: 10, top: 40 }), { left: 0, top: 40, width: 0, height: 0 });
  assert.equal(frameTaken({ mode: "hunt", hovering: false, shielded: false }), true);
  assert.equal(frameTaken({ mode: "hunt", hovering: false, shielded: true }), true, "a hunt is the scene's whatever the layer does");
  assert.equal(frameTaken({ mode: "work", hovering: false, shielded: false }), false);
  assert.equal(frameTaken({ mode: "work", hovering: true, shielded: false }), true);
  assert.equal(frameTaken({ mode: "work", hovering: true, shielded: true }), false, "a drag elsewhere keeps the pointer from the frame");
  assert.equal(modeAfter("work", "hunt"), "hunt");
  assert.equal(modeAfter("hunt", "fly"), "hunt", "a word the scene does not know changes nothing");
  assert.equal(modeAfter("work", null), "work");
});

test("a page point meets a solid region in the scene's own pixels, edges excluded on the far side", () => {
  const strip = [{ x: 960, y: 200, width: 56, height: 300 }];
  assert.equal(pointInSolid({ x: 960, y: 240 }, strip, 40), true, "the top-left corner is inside");
  assert.equal(pointInSolid({ x: 1015, y: 539 }, strip, 40), true);
  assert.equal(pointInSolid({ x: 1016, y: 300 }, strip, 40), false, "the far edge is outside");
  assert.equal(pointInSolid({ x: 970, y: 239 }, strip, 40), false, "above the region, by the chrome's height");
  assert.equal(pointInSolid({ x: 970, y: 300 }, [], 40), false);
});

test("the obstacles are the showing browser slots, from the chrome down; terminals and hidden slots are none", () => {
  const slots = [
    { rect: { left: 300, top: 60, width: 500, height: 400 }, visible: true, layer: "browser" },
    { rect: { left: 820, top: 60, width: 200, height: 400 }, visible: true, layer: "terminal" },
    { rect: { left: 0, top: 60, width: 200, height: 400 }, visible: false, layer: "browser" },
    { rect: null, visible: true, layer: "browser" },
    { rect: { left: 10, top: 20, width: 100, height: 100 }, visible: true, layer: "browser" },
  ];
  assert.deepEqual(obstaclesOf(slots, 40), [
    { x: 300, y: 20, width: 500, height: 400 },
    { x: 10, y: 0, width: 100, height: 80 },
  ]);
  assert.deepEqual(obstaclesOf([{ rect: { left: 0, top: 0, width: 10, height: 30 }, visible: true, layer: "browser" }], 40), [], "a slot wholly behind the chrome is nothing");
});

test("the solid regions become clears on the page, rounded outward with their corner, and the event carries every fact", () => {
  assert.deepEqual(solidClears([{ x: 960.4, y: 200.2, width: 55.5, height: 300.3, radius: 8 }], 40), [{ left: 960, top: 240, width: 56, height: 301, radius: 8 }]);
  assert.deepEqual(solidClears([{ x: 0, y: 0, width: 0, height: 10 }], 40), [], "an empty box is no clear");
  const event = sceneEvent({ mode: "work", viewport: vp, slots: [{ rect: { left: 300, top: 60, width: 500, height: 400 }, visible: true, layer: "browser" }], cuts: true });
  assert.deepEqual(event, { mode: "work", viewport: { width: 1024, height: 660 }, obstacles: [{ x: 300, y: 20, width: 500, height: 400 }], cuts: true });
  assert.equal(sceneKey(event), sceneKey(sceneEvent({ mode: "work", viewport: { ...vp }, slots: [{ rect: { left: 300, top: 60, width: 500, height: 400 }, visible: true, layer: "browser" }], cuts: true })), "the same facts are one key");
  assert.notEqual(sceneKey(event), sceneKey({ ...event, mode: "hunt" }));
});

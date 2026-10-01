import test from "node:test";
import assert from "node:assert/strict";
import { BOTTOM_RIGHT_CLEARANCE, DEFAULT_MIN, EDGE, STAGGER, VIEWPORT_MARGIN, clampSize, defaultPlacement, hiddenByLayer, placementAfterResize, sizeBounds, sizeFrom, windowPrefFrom } from "./addonWindowModel.mjs";

const viewport = { width: 1200, height: 800, top: 40 };
const manifest = { width: 200, height: 120, min_width: 120, min_height: 80, max_width: 400, max_height: 300 };

test("bounds are the manifest's, kept inside the window", () => {
  assert.deepEqual(sizeBounds(manifest, viewport), { minWidth: 120, minHeight: 80, maxWidth: 400, maxHeight: 300 });
  const open = sizeBounds({ width: 200, height: 120 }, viewport);
  assert.deepEqual(open, { minWidth: DEFAULT_MIN.width, minHeight: DEFAULT_MIN.height, maxWidth: 1200 - 2 * VIEWPORT_MARGIN, maxHeight: 800 - 40 - 2 * VIEWPORT_MARGIN });
  const tiny = sizeBounds(manifest, { width: 300, height: 200, top: 40 });
  assert.equal(tiny.maxWidth, 300 - 32, "never wider than the window");
  assert.ok(tiny.minWidth <= tiny.maxWidth && tiny.minHeight <= tiny.maxHeight, "the bounds stay ordered");
});

test("a size is clamped and a corner drag lands inside the bounds", () => {
  const b = sizeBounds(manifest, viewport);
  assert.deepEqual(clampSize({ width: 50, height: 5000 }, b), { width: 120, height: 300 });
  assert.deepEqual(clampSize({ width: NaN, height: "x" }, b), { width: 120, height: 80 });
  assert.deepEqual(sizeFrom({ width: 200, height: 120 }, 50, -10, b), { width: 250, height: 110 });
  assert.deepEqual(sizeFrom({ width: 200, height: 120 }, 900, 900, b), { width: 400, height: 300 });
});

test("a resize keeps the corner the person did not touch", () => {
  const right = { h: "right", x: 100, v: "bottom", y: 40 };
  assert.deepEqual(placementAfterResize(right, { width: 200, height: 120 }, { width: 260, height: 150 }), { h: "right", x: 40, v: "bottom", y: 10 });
  assert.deepEqual(placementAfterResize(right, { width: 200, height: 120 }, { width: 400, height: 400 }), { h: "right", x: 0, v: "bottom", y: 0 }, "never past the edge");
  const left = { h: "left", x: 24, v: "top", y: 60 };
  assert.deepEqual(placementAfterResize(left, { width: 200, height: 120 }, { width: 260, height: 150 }), left, "a top-left window grows away from its anchors");
});

test("windows open in their corner, staggered, and clear of the notes dock and the pet", () => {
  assert.deepEqual(defaultPlacement("top_left", 0), { h: "left", x: EDGE, v: "top", y: EDGE });
  assert.deepEqual(defaultPlacement("top_right", 2), { h: "right", x: EDGE + 2 * STAGGER, v: "top", y: EDGE + 2 * STAGGER });
  assert.deepEqual(defaultPlacement("bottom_left", 1), { h: "left", x: EDGE + STAGGER, v: "bottom", y: EDGE + STAGGER });
  assert.deepEqual(defaultPlacement("bottom_right", 0), { h: "right", x: BOTTOM_RIGHT_CLEARANCE, v: "bottom", y: EDGE });
  assert.deepEqual(defaultPlacement(undefined, 0), defaultPlacement("bottom_right", 0), "the manifest's default corner");
  const places = new Set([0, 1, 2, 3, 4, 5].map((i) => JSON.stringify(defaultPlacement("top_left", i))));
  assert.equal(places.size, 6, "six windows, six places");
});

test("a window hides only while it meets a showing browser layer", () => {
  const box = { left: 100, top: 100 };
  const size = { width: 200, height: 120 };
  const browser = { visible: true, layer: "browser", rect: { left: 150, top: 150, width: 600, height: 400 } };
  assert.equal(hiddenByLayer(box, size, [browser]), true);
  assert.equal(hiddenByLayer({ left: 900, top: 100 }, size, [browser]), false, "beside it");
  assert.equal(hiddenByLayer(box, size, [{ ...browser, layer: "terminal" }]), false, "the terminal layer is not a page");
  assert.equal(hiddenByLayer(box, size, [{ ...browser, visible: false }]), false);
  assert.equal(hiddenByLayer(box, size, [{ ...browser, rect: null }]), false);
  assert.equal(hiddenByLayer(box, size, []), false);
});

test("a stored preference is judged half by half", () => {
  const b = sizeBounds(manifest, viewport);
  const fallback = defaultPlacement("bottom_right", 0);
  const opening = { width: 200, height: 120 };
  assert.deepEqual(windowPrefFrom(null, opening, fallback, b), { dock: fallback, size: opening });
  assert.deepEqual(windowPrefFrom("{ not json", opening, fallback, b), { dock: fallback, size: opening });
  const stored = JSON.stringify({ dock: { h: "left", x: 10, v: "top", y: 50 }, size: { width: 300, height: 5000 } });
  assert.deepEqual(windowPrefFrom(stored, opening, fallback, b), { dock: { h: "left", x: 10, v: "top", y: 50 }, size: { width: 300, height: 300 } });
  const badDock = JSON.stringify({ dock: { h: "middle", x: -1 }, size: { width: 250, height: 100 } });
  assert.deepEqual(windowPrefFrom(badDock, opening, fallback, b), { dock: fallback, size: { width: 250, height: 100 } });
});

test("a window that leaves gives back every count it holds, and a page loaded afresh wears no title of the last one's", async () => {
  const { readFileSync } = await import("node:fs");
  const win = readFileSync(new URL("./AddonWindow.tsx", import.meta.url), "utf8");
  const leave = win.slice(win.indexOf("if (shielding.current) onShield(false);"), win.indexOf("const under = hiddenByLayer("));
  assert.ok(leave.includes("if (resizing.current) onShield(false);"), "a resize's count is given back at unmount, as a drag's is — a leaked count shields every frame for good");
  assert.ok(win.includes("useEffect(() => setTitle(null), [frameKey]);"), "the bar's title is reset with the frame");
});

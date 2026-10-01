/**
 * The device mirror's cadence and its marks (ide/19). Run with
 * `node --test desktop/src/views/_workbench/deviceMirrorModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { ERROR_MS, FRAME_MS, MAX_ERRORS, bootDoorWords, drawnRect, markCss, markFromDrag, mirrorCadence } from "./deviceMirrorModel.mjs";

test("the mirror polls every half second only while the device is up, the window awake and in front, and backs off after an error", () => {
  const well = { up: true, awake: true, focused: true, errors: 0 };
  assert.deepEqual(mirrorCadence(well), { poll: true, every: FRAME_MS, reason: null, stopped: false });
  assert.equal(FRAME_MS, 500);
  assert.equal(mirrorCadence({ ...well, errors: 1 }).every, ERROR_MS, "slower after a failure");
  assert.equal(mirrorCadence({ ...well, errors: MAX_ERRORS }).poll, false, "and it stops after enough");
  assert.ok(mirrorCadence({ ...well, errors: MAX_ERRORS }).reason.includes("stopped answering"));
  // The screen offers *Try again* on the flag — never by reading the reason's words, which are the window's language.
  assert.equal(mirrorCadence({ ...well, errors: MAX_ERRORS }).stopped, true);
  for (const pause of [{ up: false }, { awake: false }, { focused: false }]) assert.equal(mirrorCadence({ ...well, ...pause }).stopped, false);
  assert.equal(mirrorCadence({ ...well, up: false }).poll, false);
  assert.equal(mirrorCadence({ ...well, up: false }).reason, "the device is shut down");
  assert.equal(mirrorCadence({ ...well, awake: false }).reason, "the window is hidden");
  assert.equal(mirrorCadence({ ...well, focused: false }).reason, "the window is in the background");
});

test("a drag maps to device pixels through the picture's drawn box, clamped, and a click is no mark", () => {
  // A 1170×2532 screen drawn in a 400×800 element: scale 0.316, letterboxed left and right.
  const box = { clientWidth: 400, clientHeight: 800, naturalWidth: 1170, naturalHeight: 2532 };
  const drawn = drawnRect(box);
  assert.ok(Math.abs(drawn.scale - 800 / 2532) < 1e-9);
  assert.ok(drawn.left > 0 && drawn.top === 0, "letterboxed on the sides");
  const mark = markFromDrag({ x: drawn.left + 10, y: 20 }, { x: drawn.left + 60, y: 120 }, box);
  assert.deepEqual(mark, { x: Math.round(10 / drawn.scale), y: Math.round(20 / drawn.scale), width: Math.round(60 / drawn.scale) - Math.round(10 / drawn.scale), height: Math.round(120 / drawn.scale) - Math.round(20 / drawn.scale) });
  assert.equal(markFromDrag({ x: 100, y: 100 }, { x: 101, y: 101 }, box), null, "a click");
  const clamped = markFromDrag({ x: -50, y: -50 }, { x: 1000, y: 2000 }, box);
  assert.deepEqual(clamped, { x: 0, y: 0, width: 1170, height: 2532 }, "clamped to the screen");
  assert.equal(markFromDrag({ x: 0, y: 0 }, { x: 10, y: 10 }, { ...box, naturalWidth: 0 }), null, "no picture yet");
  // And back to a box over the element.
  const css = markCss({ x: 0, y: 0, width: 1170, height: 2532 }, box);
  assert.ok(Math.abs(css.left - drawn.left) < 1e-9 && css.top === 0 && Math.abs(css.height - 800) < 1e-9);
  assert.equal(markCss({ x: 1, y: 1, width: 1, height: 1 }, { ...box, clientWidth: 0 }), null);
});

test("the boot door says what to do for a simulator, an emulator and a phone", () => {
  assert.deepEqual(bootDoorWords({ name: "iPhone 16", kind: "simulator", state: "shutdown" }).verb, "Boot");
  assert.equal(bootDoorWords({ name: "Pixel 8", kind: "emulator", state: "shutdown" }).verb, "Start");
  const phone = bootDoorWords({ name: "SM G991B", kind: "physical", state: "offline" });
  assert.equal(phone.verb, null);
  assert.ok(phone.title.endsWith("is offline") && phone.hint.includes("trust this computer"));
});

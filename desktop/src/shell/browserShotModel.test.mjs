/**
 * A screenshot's facts. Run with `node --test desktop/src/shell/browserShotModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { NOT_SHOWN as AGENT_NOT_SHOWN } from "./browserBridgeModel.mjs";
import { CLIPBOARD_REFUSED, DEFAULT_SHOT_WIDTH, MAX_SHOT_WIDTH, MIN_SHOT_WIDTH, NOT_SHOWN_WORDS, pngSize, shotName, shotWidth, shotWords } from "./browserShotModel.mjs";

/** A PNG header with this size and nothing behind it. */
function png(width, height) {
  const bytes = new Uint8Array(33);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  const be = (at, n) => {
    bytes[at] = (n >>> 24) & 0xff;
    bytes[at + 1] = (n >>> 16) & 0xff;
    bytes[at + 2] = (n >>> 8) & 0xff;
    bytes[at + 3] = n & 0xff;
  };
  be(16, width);
  be(20, height);
  return bytes;
}

test("the width is the setting within bounds, else the default", () => {
  assert.equal(shotWidth(1920), 1920);
  assert.equal(shotWidth("1440"), 1440);
  assert.equal(shotWidth(10), MIN_SHOT_WIDTH);
  assert.equal(shotWidth(99999), MAX_SHOT_WIDTH);
  assert.equal(shotWidth(null), DEFAULT_SHOT_WIDTH);
  assert.equal(shotWidth("wide"), DEFAULT_SHOT_WIDTH);
  assert.equal(shotWidth(1280.6), 1281);
  assert.equal(shotWidth(undefined), DEFAULT_SHOT_WIDTH, "unset is the default");
  assert.equal(shotWidth(""), DEFAULT_SHOT_WIDTH, "an empty field is the default, not the minimum");
  assert.equal(shotWidth(true), DEFAULT_SHOT_WIDTH, "a boolean is not a width");
  assert.equal(shotWidth(Number.NaN), DEFAULT_SHOT_WIDTH);
  assert.equal(shotWidth(Number.POSITIVE_INFINITY), DEFAULT_SHOT_WIDTH);
  assert.equal(shotWidth(-50), MIN_SHOT_WIDTH, "a negative number is clamped up");
  assert.equal(shotWidth(0), MIN_SHOT_WIDTH, "a zero someone typed is a number, clamped");
});

test("a screenshot is named after its tab and the moment, as a PNG", () => {
  const at = new Date("2026-09-14T10:20:30.456Z");
  assert.equal(shotName("b7", at), "browser-b7-20260914T102030Z.png");
  assert.equal(shotName("../x", at), "browser-x-20260914T102030Z.png", "only word characters of the key survive");
  assert.equal(shotName("", at), "browser-tab-20260914T102030Z.png");
});

test("a PNG says its size in its header; anything else says nothing", () => {
  assert.deepEqual(pngSize(png(1280, 800)), { width: 1280, height: 800 });
  assert.deepEqual(pngSize(png(70000, 3)), { width: 70000, height: 3 }, "big widths read unsigned");
  assert.equal(pngSize(png(0, 800)), null);
  assert.equal(pngSize(new Uint8Array([1, 2, 3])), null);
  const notPng = png(10, 10);
  notPng[0] = 0x47;
  assert.equal(pngSize(notPng), null);
  const noIhdr = png(10, 10);
  noIhdr[12] = 0x4a;
  assert.equal(pngSize(noIhdr), null);
  assert.equal(pngSize(null), null);
  assert.equal(shotWords({ width: 1280, height: 800 }), "1280×800 PNG");
});

test("a shot of a page that is not showing is refused in the person's words, apart from the agent's, and a refused clipboard is said", () => {
  assert.match(NOT_SHOWN_WORDS, /not showing/, "what the toast says");
  assert.match(NOT_SHOWN_WORDS, /try again/, "and what to do");
  assert.notEqual(NOT_SHOWN_WORDS, AGENT_NOT_SHOWN, "the agent reads the bridge's sentence, the person the camera's");
  assert.match(CLIPBOARD_REFUSED, /clipboard/);
});

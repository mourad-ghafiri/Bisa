import test from "node:test";
import assert from "node:assert/strict";
import { MEDIA_STATES, mediaKey, pageRect, playerPath, playerShown, stateOf } from "./addonMediaModel.mjs";

test("the player's box is the frame's plus the addon's, clamped inside it", () => {
  const frame = { left: 100, top: 50, width: 440, height: 300 };
  assert.deepEqual(pageRect(frame, { x: 10, y: 20, width: 320, height: 200 }), { left: 110, top: 70, width: 320, height: 200 });
  assert.deepEqual(pageRect(frame, { x: 200, y: 20, width: 320, height: 200 }), { left: 300, top: 70, width: 240, height: 200 }, "cut at the frame's right edge");
  assert.equal(pageRect(frame, { x: 300, y: 20, width: 320, height: 200 }), null, "what is left is under the square");
  assert.equal(pageRect(frame, { x: 0, y: 150, width: 320, height: 200 }), null, "too short at the bottom");
  assert.equal(pageRect({ ...frame, width: 0 }, { x: 0, y: 0, width: 320, height: 200 }), null, "a frame with no size");
  assert.deepEqual(pageRect({ left: 0.4, top: 0.6, width: 300, height: 260 }, { x: 0, y: 0, width: 300, height: 260 }), { left: 0, top: 1, width: 300, height: 260 }, "whole pixels");
  assert.equal(mediaKey("media-player"), "addon-media:media-player");
  assert.equal(playerPath("media-player", "dQw4w9WgXcQ"), "/addons/media-player/player/youtube/dQw4w9WgXcQ");
});

test("the player hides on a drag, under a surface and off screen", () => {
  const rect = { left: 0, top: 0, width: 320, height: 200 };
  const shown = { embedded: true, visible: true, shielded: false, mayShow: true, rect };
  assert.equal(playerShown(shown), true);
  assert.equal(playerShown({ ...shown, embedded: false }), false);
  assert.equal(playerShown({ ...shown, visible: false }), false);
  assert.equal(playerShown({ ...shown, shielded: true }), false);
  assert.equal(playerShown({ ...shown, mayShow: false }), false);
  assert.equal(playerShown({ ...shown, rect: null }), false);
});

test("the page's word is read as the media event, and nothing else is", () => {
  assert.equal(stateOf(null), null);
  assert.equal(stateOf("bisa:media"), null);
  assert.equal(stateOf({ type: "bisa:pick", selector: "#a" }), null, "the inspector's words are not the player's");
  assert.deepEqual(stateOf({ type: "bisa:media", state: "playing", title: "A song" }), { state: "playing", title: "A song" });
  assert.deepEqual(stateOf({ type: "bisa:media", state: "dancing" }), { state: "unstarted", title: "" });
  assert.deepEqual(stateOf({ type: "bisa:media", state: "error", code: "150", title: "x", words: "Its owner does not let this video play outside YouTube." }), {
    state: "error",
    title: "x",
    code: "150",
    words: "Its owner does not let this video play outside YouTube.",
  });
  assert.deepEqual(stateOf({ type: "bisa:media", state: "error" }), { state: "error", title: "", code: "", words: "" });
  assert.equal(stateOf({ type: "bisa:media", state: "ready", title: "t".repeat(80) }).title.length, 60);
  assert.ok(MEDIA_STATES.includes("ended") && Object.isFrozen(MEDIA_STATES));
});

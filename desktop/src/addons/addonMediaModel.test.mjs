import test from "node:test";
import assert from "node:assert/strict";
import { MEDIA_STATES, OPEN_RETRIES, OPEN_RETRY_MS, enqueue, mediaKey, pageRect, playerPath, playerShown, stateOf } from "./addonMediaModel.mjs";

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

test("one key's opens and closes run in order whoever queued them, a throw stops nothing, and a settled tail is forgotten", async () => {
  const chains = new Map();
  const ran = [];
  let release;
  const held = new Promise((r) => (release = r));
  // The old instance's close is still out when the new instance's open is queued.
  const close = enqueue(chains, "addon-media:a", async () => {
    await held;
    ran.push("close");
  });
  const open = enqueue(chains, "addon-media:a", () => {
    ran.push("open");
  });
  // Another key is nobody's wait.
  await enqueue(chains, "addon-media:b", () => {
    ran.push("b");
  });
  assert.deepEqual(ran, ["b"]);
  release();
  await open;
  assert.deepEqual(ran, ["b", "close", "open"]);
  await close;
  // A step that throws is swallowed and the next still runs.
  await enqueue(chains, "addon-media:a", () => {
    throw new Error("refused");
  });
  await enqueue(chains, "addon-media:a", () => {
    ran.push("after");
  });
  assert.deepEqual(ran, ["b", "close", "open", "after"]);
  await new Promise((r) => setImmediate(r));
  assert.equal(chains.size, 0, "a settled tail is forgotten");
  assert.ok(OPEN_RETRIES >= 1 && OPEN_RETRY_MS > 0);
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

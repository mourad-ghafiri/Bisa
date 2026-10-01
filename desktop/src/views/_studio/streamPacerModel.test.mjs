/**
 * The reveal pacer: a frame's backlog spread over the engine's interval.
 * Run with `node --test desktop/src/views/_studio/streamPacerModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { FRAME_MS, JUMP_CHARS, MIN_RATE, pace, reveal } from "./streamPacerModel.mjs";

test("a backlog is revealed over one interval — no faster, and always caught up by the next frame", () => {
  // 120 chars behind: 16 ms of a 120 ms interval shows 16 of them.
  assert.equal(pace({ shown: 0, target: 120, dtMs: 16 }), 16);
  // Step by step, the whole interval reveals the whole backlog: what is left
  // is spread over the time left since the frame landed.
  let shown = 0;
  for (let t = 0; t < FRAME_MS; t += 16) shown = pace({ shown, target: 120, dtMs: 16, sinceMs: t });
  assert.equal(shown, 120);
  // Past the interval, whatever is left is shown at once.
  assert.equal(pace({ shown: 40, target: 120, dtMs: 16, sinceMs: FRAME_MS + 5 }), 120);
  // A pause (a hidden tab) catches up in one step.
  assert.equal(pace({ shown: 10, target: 500, dtMs: 5000 }), 500);
});

test("caught up shows nothing more, and never past the target", () => {
  assert.equal(pace({ shown: 40, target: 40, dtMs: 16 }), 40);
  assert.equal(pace({ shown: 50, target: 40, dtMs: 16 }), 40, "a target that shrank (a prime) snaps to it");
  assert.equal(pace({ shown: 0, target: 0, dtMs: 16 }), 0);
});

test("a trickle still moves: at least one char a frame, at least the floor rate", () => {
  assert.equal(pace({ shown: 0, target: 3, dtMs: 1 }), 1);
  assert.ok(MIN_RATE * 1000 >= 100, "at least a hundred chars a second");
});

test("a backlog past the bound, or a turn that ended, is shown at once", () => {
  assert.equal(pace({ shown: 0, target: JUMP_CHARS + 1, dtMs: 16 }), JUMP_CHARS + 1);
  assert.equal(pace({ shown: 0, target: 300, dtMs: 16, done: true }), 300);
});

test("a reveal never splits a surrogate pair", () => {
  const text = "ok 😀 yes";
  assert.equal(reveal(text, 4), "ok ", "the high half alone is held back one frame");
  assert.equal(reveal(text, 5), "ok 😀");
  assert.equal(reveal(text, 99), text);
  assert.equal(reveal(text, -1), "");
});

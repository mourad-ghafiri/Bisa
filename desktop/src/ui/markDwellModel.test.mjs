/**
 * The beat a session's mark dwells on a working word, tested where the rule
 * lives. No DOM; the timer is the component's.
 *
 * Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { DWELL_MS, shownState } from "./markDwellModel.mjs";

const running = (tool) => ({ state: "running", tool, args: "", tier: "read" });
const thinking = { state: "thinking" };
const waiting = { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: null } };

test("a working word holds for a beat against another working word, then gives way", () => {
  assert.deepEqual(shownState(thinking, running("Read"), 40), { state: thinking, holdMs: DWELL_MS - 40 }, "the breath is not cut by a spin 40 ms in");
  assert.deepEqual(shownState(thinking, running("Read"), DWELL_MS), { state: running("Read"), holdMs: 0 }, "held its beat: the spin shows");
  assert.deepEqual(shownState(running("Read"), thinking, 10), { state: running("Read"), holdMs: DWELL_MS - 10 });
  assert.deepEqual(shownState({ state: "starting" }, thinking, 0), { state: { state: "starting" }, holdMs: DWELL_MS }, "starting is a working word too");
});

test("attention and an end show at once, whatever the mark was doing — and so does the first word", () => {
  assert.deepEqual(shownState(running("Read"), waiting, 5), { state: waiting, holdMs: 0 }, "a raised hand is never delayed");
  assert.deepEqual(shownState(thinking, { state: "failed", reason: "x" }, 5), { state: { state: "failed", reason: "x" }, holdMs: 0 });
  assert.deepEqual(shownState(thinking, { state: "done" }, 5), { state: { state: "done" }, holdMs: 0 });
  assert.deepEqual(shownState(waiting, thinking, 5), { state: thinking, holdMs: 0 }, "and leaving the hand shows at once: the person answered");
  assert.deepEqual(shownState(null, thinking, 0), { state: thinking, holdMs: 0 }, "the first word has nothing to dwell on");
  assert.deepEqual(shownState("idle", "thinking", 5), { state: "thinking", holdMs: 0 }, "words alone work too; idle is not a working word");
});

test("the same word in a new object is the new object at once: same glyph, same motion, nothing restarts", () => {
  const next = running("Edit");
  assert.deepEqual(shownState(running("Read"), next, 5), { state: next, holdMs: 0 });
  assert.equal(shownState(running("Read"), next, 5).state, next, "the newer object — its args are the row's words");
});

test("the mark applies the rule through one hook, and only the mark", () => {
  const src = readFileSync(new URL("./SessionMark.tsx", import.meta.url), "utf8");
  assert.ok(src.includes("shownState("), "the mark asks the model");
  assert.ok(src.includes("window.setTimeout(") && src.includes("window.clearTimeout("), "and holds the beat with a timer it clears");
});

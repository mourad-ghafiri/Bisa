import assert from "node:assert/strict";
import { test } from "node:test";

import { NOTHING_ENDED, endedOf, endedParts, endedWords, saidAfter, stopTone, willStopWords } from "./stopOutcomeModel.mjs";

test("the node's block is read tolerantly, from the answer or under its retirement, and an older node says nothing", () => {
  assert.deepEqual(endedOf({ stopped: "r1", ended: { sessions: 2, terminated: 1, still_live: 0, children: ["g2"] } }), { sessions: 2, terminated: 1, still_live: 0, children: ["g2"] });
  assert.deepEqual(endedOf({ retired: { ended: { sessions: 1, terminated: 0, still_live: 0, children: [] } } }), { sessions: 1, terminated: 0, still_live: 0, children: [] });
  assert.equal(endedOf({ stopped: "r1", withdrawn: [] }), null, "an older node's answer carries no block");
  assert.equal(endedOf(null), null);
  assert.deepEqual(endedOf({ ended: { sessions: "x", children: "y" } }), { sessions: 0, terminated: 0, still_live: 0, children: [] }, "nonsense reads as nothing");
});

test("every part alone and together, in the one order: stopped, terminated, could not be ended, spawned goals", () => {
  assert.deepEqual(endedParts(NOTHING_ENDED), []);
  assert.deepEqual(endedParts({ sessions: 1, terminated: 0, still_live: 0, children: [] }), ["1 session stopped"]);
  assert.deepEqual(endedParts({ sessions: 3, terminated: 1, still_live: 2, children: ["a", "b"] }), [
    "3 sessions stopped",
    "1 harness did not answer and was terminated",
    "2 sessions could not be ended — see Agents",
    "2 spawned goals stopped too",
  ]);
  assert.deepEqual(endedParts({ sessions: 3, terminated: 2, still_live: 1, children: ["a"] }, { closed: true, sessionsSaid: true }), [
    "2 harnesses did not answer and were terminated",
    "1 session could not be ended — see Agents",
    "1 spawned goal closed too",
  ]);
  assert.equal(endedWords(NOTHING_ENDED), null);
  assert.equal(endedWords({ sessions: 1, terminated: 1, still_live: 0, children: [] }), "1 session stopped and 1 harness did not answer and was terminated");
  assert.equal(endedWords({ sessions: 1, terminated: 1, still_live: 0, children: ["a"] }), "1 session stopped, 1 harness did not answer and was terminated and 1 spawned goal stopped too");
});

test("the toast is the lead, and after it what was ended — the lead alone when nothing was, the words alone when there is no lead", () => {
  assert.equal(saidAfter("Stopped.", null), "Stopped.", "an older node: the lead as it was");
  assert.equal(saidAfter("Stopped.", NOTHING_ENDED), "Stopped.");
  assert.equal(saidAfter("Stopped.", { sessions: 2, terminated: 0, still_live: 0, children: [] }), "Stopped. — 2 sessions stopped");
  assert.equal(saidAfter("", { sessions: 0, terminated: 1, still_live: 0, children: [] }), "1 harness did not answer and was terminated");
  assert.equal(stopTone(null), "ok");
  assert.equal(stopTone({ sessions: 1, terminated: 1, still_live: 0, children: [] }), "ok", "a termination is the stop doing its job");
  assert.equal(stopTone({ sessions: 1, terminated: 0, still_live: 1, children: [] }), "info", "what could not be ended is a note, never a success");
});

test("a confirm says what will stop: the sessions on the thing, the goals it spawned — stopped, or closed with it", () => {
  assert.equal(willStopWords(), null);
  assert.equal(willStopWords({ sessions: 1 }), "The 1 session working on it ends.");
  assert.equal(willStopWords({ sessions: 2, children: 1 }), "The 2 sessions working on it end. The 1 goal it spawned is stopped with it.");
  assert.equal(willStopWords({ children: 3, closing: true }), "The 3 goals it spawned are closed with it.");
});

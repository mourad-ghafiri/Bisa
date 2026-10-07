/**
 * The *agent writing* dot: the bus's hint, held to the roster's word. Run
 * with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { clearedByRow, rebuilt, sameWorking, withReplied, withThinking } from "./workingModel.mjs";

const turn = (id, state, over = {}) => ({ id, kind: "conversation", agent: "scout", state: { state }, origin: { origin: "turn", scope: "c1" }, ...over });

test("a hint sets, a reply clears, and nothing changes the map that already says so", () => {
  const w1 = withThinking({}, "c1", "scout");
  assert.deepEqual(w1, { c1: ["scout"] });
  assert.equal(withThinking(w1, "c1", "scout"), w1, "idempotent: the same object");
  assert.deepEqual(withThinking(w1, "c1", "ada"), { c1: ["scout", "ada"] });
  assert.deepEqual(withReplied(w1, "c1", "scout"), { c1: [] });
  assert.equal(withReplied(w1, "c1", "ada"), w1, "an agent not in the scope: nothing to clear");
  assert.equal(withThinking(w1, "", "scout"), w1);
});

test("a row that ended, parked or went idle clears its agent from its scope; a busy one does not", () => {
  const w = { c1: ["scout", "ada"] };
  assert.deepEqual(clearedByRow(w, turn("t1", "done")), { c1: ["ada"] });
  assert.deepEqual(clearedByRow(w, turn("t1", "parked")), { c1: ["ada"] });
  assert.deepEqual(clearedByRow(w, turn("t1", "idle")), { c1: ["ada"] });
  assert.equal(clearedByRow(w, turn("t1", "thinking")), w, "still writing");
  assert.equal(clearedByRow(w, turn("t1", "done", { agent: null })), w, "a row with no agent names nobody");
  assert.deepEqual(clearedByRow(w, { kind: "worker", agent: "scout", goal: "c1", state: { state: "failed" } }), { c1: ["ada"] }, "a worker's scope is its goal");
});

test("a roster read whole rebuilds the map from its busy rows alone: a lost reply stops showing", () => {
  const rows = [turn("t1", "thinking"), turn("t2", "running", { agent: "ada", origin: { origin: "turn", scope: "c2" } }), turn("t3", "done", { agent: "grace", origin: { origin: "turn", scope: "c3" } }), { kind: "guided", agent: "workflow-agent", goal: "g1", state: { state: "thinking" } }];
  assert.deepEqual(rebuilt(rows), { c1: ["scout"], c2: ["ada"], g1: ["workflow-agent"] });
  assert.deepEqual(rebuilt([]), {}, "nothing busy, nothing working — whatever a lost frame left");
  assert.equal(sameWorking({ c1: ["scout"], c2: [] }, { c1: ["scout"] }), true, "an empty scope is no scope");
  assert.equal(sameWorking({ c1: ["scout"] }, { c1: ["ada"] }), false);
  assert.equal(sameWorking(null, {}), true);
});

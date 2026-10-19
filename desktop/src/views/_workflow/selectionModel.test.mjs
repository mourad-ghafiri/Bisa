/**
 * The selection as one value the canvas and the panel share. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/selectionModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import {
  edgeIdOf,
  flowFacts,
  flowRef,
  flowRefAfterRelabel,
  flowRefOf,
  flowRefOfEdgeId,
  keptSelection,
  propertiesScrollName,
  readSelection,
  sameSelection,
  selectedIds,
  selectionAfterDelete,
  selectionOf,
  stepOf,
  stepSelection,
  storedSelection,
} from "./selectionModel.mjs";
import { toGraph } from "./workflowGraph.mjs";

/** start → gate (yes → work, otherwise no → done) → work (on failure → mend; a timeout divert → done; loops back to gate) → mend → done. */
const wf = {
  steps: [
    { id: "start", name: "Start", kind: "start", on: { event: "manual" }, then: [{ to: "gate" }] },
    { id: "gate", name: "Gate", kind: "decide", rules: [{ when: { condition: "answered", step: "work" }, branch: "yes" }], otherwise: "no", then: [{ to: "work", branch: "yes" }, { to: "done", branch: "no" }] },
    { id: "work", name: "Work", kind: "agent", instructions: "x", boundaries: [{ name: "timeout", on: { on: "after", secs: 5 }, act: "divert" }], on_fail: { on_fail: "then", step: "mend" }, then: [{ to: "done" }, { to: "done", branch: "timeout" }, { to: "gate" }] },
    { id: "mend", name: "Mend", kind: "human", prompt: "?", then: [{ to: "done" }] },
    { id: "done", name: "Done", kind: "end" },
  ],
};

test("a step, a flow, a many: the three shapes, and what each is", () => {
  assert.deepEqual(stepSelection("work"), { kind: "step", id: "work" });
  assert.equal(stepSelection(""), null);
  assert.equal(stepSelection(null), null);
  assert.equal(stepOf(stepSelection("work")), "work");
  assert.equal(stepOf(flowRef("a", "b")), null);
  assert.equal(stepOf(null), null);
  assert.deepEqual(flowRef("a", "b"), { kind: "flow", from: "a", to: "b", branch: null, route: "then" });
  assert.deepEqual(flowRef("a", "b", "yes"), { kind: "flow", from: "a", to: "b", branch: "yes", route: "then" });
  assert.deepEqual(flowRef("a", "b", "yes", "on_fail"), { kind: "flow", from: "a", to: "b", branch: null, route: "on_fail" }, "a failure route has no branch");
});

test("a flow's id is the graph's edge id, both ways, for every edge the graph draws", () => {
  const { edges } = toGraph(wf);
  assert.equal(edges.length, 8);
  for (const e of edges) {
    const ref = flowRefOf(e);
    assert.equal(edgeIdOf(ref), e.id, `round-trips ${e.id}`);
    assert.deepEqual(flowRefOfEdgeId(e.id), ref, `parses ${e.id}`);
  }
  assert.deepEqual(flowRefOfEdgeId("work!>mend"), flowRef("work", "mend", null, "on_fail"));
  assert.deepEqual(flowRefOfEdgeId("gate->work#yes"), flowRef("gate", "work", "yes"));
  for (const bad of ["", "->b", "a->", "a->b#", "!>b", "a!>", "a!>b#x", 7, null, "a-b"]) assert.equal(flowRefOfEdgeId(bad), null, `no flow is ${JSON.stringify(bad)}`);
});

test("what the canvas reports picked becomes one value: nothing, one, or many — and back to ids", () => {
  assert.equal(selectionOf({ nodes: [], edges: [] }), null);
  assert.equal(selectionOf(null), null);
  assert.deepEqual(selectionOf({ nodes: ["work"], edges: [] }), stepSelection("work"));
  assert.deepEqual(selectionOf({ nodes: [], edges: ["gate->work#yes"] }), flowRef("gate", "work", "yes"));
  const many = selectionOf({ nodes: ["work", "mend"], edges: ["work!>mend", "nonsense"] });
  assert.deepEqual(many, { kind: "many", steps: ["work", "mend"], flows: [flowRef("work", "mend", null, "on_fail")] }, "an id no flow of ours has is dropped");
  assert.deepEqual(selectionOf({ nodes: [], edges: ["nonsense"] }), null);
  assert.deepEqual(selectedIds(many), { nodes: ["work", "mend"], edges: ["work!>mend"] });
  assert.deepEqual(selectedIds(stepSelection("work")), { nodes: ["work"], edges: [] });
  assert.deepEqual(selectedIds(flowRef("gate", "done", "no")), { nodes: [], edges: ["gate->done#no"] });
  assert.deepEqual(selectedIds(null), { nodes: [], edges: [] });
});

test("two selections are the same by what they pick", () => {
  assert.ok(sameSelection(null, null));
  assert.ok(sameSelection(stepSelection("a"), stepSelection("a")));
  assert.ok(!sameSelection(stepSelection("a"), stepSelection("b")));
  assert.ok(!sameSelection(stepSelection("a"), null));
  assert.ok(!sameSelection(stepSelection("a"), flowRef("a", "b")));
  assert.ok(sameSelection(flowRef("a", "b", "x"), flowRef("a", "b", "x")));
  assert.ok(!sameSelection(flowRef("a", "b", "x"), flowRef("a", "b", "y")));
  assert.ok(sameSelection({ kind: "many", steps: ["a"], flows: [flowRef("a", "b")] }, { kind: "many", steps: ["a"], flows: [flowRef("a", "b")] }));
  assert.ok(!sameSelection({ kind: "many", steps: ["a"], flows: [] }, { kind: "many", steps: ["a", "b"], flows: [] }));
  assert.ok(!sameSelection({ kind: "many", steps: ["a"], flows: [flowRef("a", "b")] }, { kind: "many", steps: ["a"], flows: [flowRef("a", "c")] }));
});

test("a selection is kept only while the workflow has it, and the same object comes back when all of it is there", () => {
  const step = stepSelection("work");
  assert.equal(keptSelection(step, wf), step);
  assert.equal(keptSelection(stepSelection("gone"), wf), null);
  assert.equal(keptSelection(null, wf), null);
  const flow = flowRef("gate", "work", "yes");
  assert.equal(keptSelection(flow, wf), flow);
  assert.equal(keptSelection(flowRef("gate", "work", "maybe"), wf), null, "a branch the gate has no flow on");
  const fail = flowRef("work", "mend", null, "on_fail");
  assert.equal(keptSelection(fail, wf), fail, "the failure route is a flow the graph has");
  const many = { kind: "many", steps: ["work", "gone"], flows: [flow, flowRef("x", "y")] };
  assert.deepEqual(keptSelection(many, wf), { kind: "many", steps: ["work"], flows: [flow] });
  assert.deepEqual(keptSelection({ kind: "many", steps: ["gone"], flows: [flow] }, wf), flow, "one flow left is a flow");
  assert.deepEqual(keptSelection({ kind: "many", steps: ["work"], flows: [flowRef("x", "y")] }, wf), stepSelection("work"), "one step left is a step");
  assert.equal(keptSelection({ kind: "many", steps: ["gone"], flows: [] }, wf), null);
  const whole = { kind: "many", steps: ["work"], flows: [flow] };
  assert.equal(keptSelection(whole, wf), whole, "nothing lost, the same object");
  assert.equal(keptSelection(step, null), null, "no workflow, nothing kept");
});

test("after a delete, what remains stays picked: a step whose flow was cut, never a flow whose step went", () => {
  const step = stepSelection("work");
  assert.equal(selectionAfterDelete(step, [], ["work->done"]), step, "an edge-only delete keeps the step");
  assert.equal(selectionAfterDelete(step, ["work"], []), null);
  assert.equal(selectionAfterDelete(null, ["work"], []), null);
  const flow = flowRef("work", "done");
  assert.equal(selectionAfterDelete(flow, [], ["work->done"]), null);
  assert.equal(selectionAfterDelete(flow, ["done"], []), null, "its target went");
  assert.equal(selectionAfterDelete(flow, ["work"], []), null, "its source went");
  assert.equal(selectionAfterDelete(flow, ["mend"], ["gate->work#yes"]), flow);
  const many = { kind: "many", steps: ["work", "mend"], flows: [flow] };
  assert.deepEqual(selectionAfterDelete(many, [], ["work->done"]), { kind: "many", steps: ["work", "mend"], flows: [] });
  assert.deepEqual(selectionAfterDelete(many, ["mend"], ["work->done"]), stepSelection("work"));
  assert.equal(selectionAfterDelete(many, ["x"], []), many, "nothing of it went: the same object");
});

test("what the memory gives back: a bare id is a step, a shape is read by its kind, a many is nothing", () => {
  assert.deepEqual(readSelection("build"), stepSelection("build"));
  assert.equal(readSelection(""), null);
  assert.equal(readSelection("x".repeat(513)), null);
  assert.deepEqual(readSelection({ kind: "step", id: "build" }), stepSelection("build"));
  assert.equal(readSelection({ kind: "step", id: 7 }), null);
  assert.deepEqual(readSelection({ kind: "flow", from: "a", to: "b" }), flowRef("a", "b"));
  assert.deepEqual(readSelection({ kind: "flow", from: "a", to: "b", branch: "yes", route: "then" }), flowRef("a", "b", "yes"));
  assert.deepEqual(readSelection({ kind: "flow", from: "a", to: "b", route: "on_fail" }), flowRef("a", "b", null, "on_fail"));
  for (const bad of [{ kind: "flow", from: "a" }, { kind: "flow", from: "a", to: "b", route: "sideways" }, { kind: "flow", from: "a", to: "b", branch: 3 }, { kind: "flow", from: "", to: "b" }, { kind: "many", steps: ["a"], flows: [] }, { kind: "other" }, 7, null, undefined, []]) {
    assert.equal(readSelection(bad), null, `nothing is read from ${JSON.stringify(bad)}`);
  }
  assert.equal(storedSelection(stepSelection("build")), "build", "a step is kept as the bare id it always was");
  assert.equal(storedSelection(flowRef("a", "b")), null, "a flow is a moment's pick");
  assert.equal(storedSelection({ kind: "many", steps: ["a"], flows: [] }), null);
  assert.equal(storedSelection(null), null);
});

test("each page of the properties pane keeps its scroll under its own name, a step's as it was always spelt", () => {
  assert.equal(propertiesScrollName(stepSelection("build")), "properties:build");
  assert.equal(propertiesScrollName(null), "properties:");
  assert.equal(propertiesScrollName(flowRef("a", "b", "yes")), "properties:flow:a->b#yes");
  assert.equal(propertiesScrollName({ kind: "many", steps: ["a"], flows: [] }), "properties:many");
});

test("a picked flow follows its branch when the branch is renamed, and nothing else moves", () => {
  const flow = flowRef("gate", "work", "yes");
  assert.deepEqual(flowRefAfterRelabel(flow, "gate", "yes", "go"), flowRef("gate", "work", "go"));
  assert.equal(flowRefAfterRelabel(flow, "gate", "no", "stop"), flow, "another branch");
  assert.equal(flowRefAfterRelabel(flow, "work", "yes", "go"), flow, "another step");
  const step = stepSelection("gate");
  assert.equal(flowRefAfterRelabel(step, "gate", "yes", "go"), step);
  assert.equal(flowRefAfterRelabel(null, "gate", "yes", "go"), null);
  const fail = flowRef("work", "mend", null, "on_fail");
  assert.equal(flowRefAfterRelabel(fail, "work", null, "x"), fail, "a failure route has no branch to rename");
});

test("a flow's facts: its ends, its word, a loop, a boundary's path, and whether its branch may be renamed here", () => {
  const plain = flowFacts(wf, flowRef("start", "gate"));
  assert.deepEqual([plain.exists, plain.from.id, plain.to.id, plain.word, plain.loop, plain.boundary, plain.relabel], [true, "start", "gate", "Then", false, false, false]);
  const branch = flowFacts(wf, flowRef("gate", "work", "yes"));
  assert.deepEqual([branch.word, branch.relabel, branch.loop], ["Branch yes", true, false]);
  const otherwise = flowFacts(wf, flowRef("gate", "done", "no"));
  assert.deepEqual([otherwise.word, otherwise.relabel], ["Otherwise", false], "the otherwise is a field of the gate, not a rule's name");
  const divert = flowFacts(wf, flowRef("work", "done", "timeout"));
  assert.deepEqual([divert.word, divert.boundary, divert.relabel], ["Boundary timeout", true, false]);
  const fail = flowFacts(wf, flowRef("work", "mend", null, "on_fail"));
  assert.deepEqual([fail.word, fail.route, fail.relabel, fail.exists], ["On failure", "on_fail", false, true]);
  const loop = flowFacts(wf, flowRef("work", "gate"));
  assert.deepEqual([loop.loop, loop.word], [true, "Then"], "a flow back to an earlier step is a loop");
  const gone = flowFacts(wf, flowRef("mend", "gate"));
  assert.deepEqual([gone.exists, gone.from.id, gone.to.id], [false, "mend", "gate"], "a flow the workflow no longer has still names its ends");
  assert.deepEqual([flowFacts(null, flowRef("a", "b")).exists, flowFacts(null, flowRef("a", "b")).from], [false, null]);
  // A divert's name on a step that branches is a branch of the step's, never a boundary's path (`toGraph`'s rule).
  const gated = { steps: [{ id: "g", name: "G", kind: "decide", rules: [{ when: { condition: "answered", step: "x" }, branch: "timeout" }], otherwise: "no", boundaries: [{ name: "timeout", on: { on: "after", secs: 1 }, act: "divert" }], then: [{ to: "e", branch: "timeout" }] }, { id: "e", name: "E", kind: "end" }] };
  assert.deepEqual([flowFacts(gated, flowRef("g", "e", "timeout")).boundary, flowFacts(gated, flowRef("g", "e", "timeout")).relabel], [false, true]);
});

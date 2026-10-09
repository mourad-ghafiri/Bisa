/**
 * The designer's edits, tested where they live.
 *
 * Nothing here renders. What can be wrong in a way a person notices: a
 * connection the validator would refuse being accepted, a rename that leaves
 * a placeholder behind, a removed step whose flows survive it.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  addStep,
  connect,
  disconnect,
  DUPLICATE_OFFSET,
  duplicateStep,
  failChoice,
  failTargets,
  freshBranch,
  incoming,
  isBranching,
  mayEdit,
  upstreamOf,
  relabelBranch,
  removeStep,
  renameInput,
  renameStep,
  replaceStep,
  setOnFail,
  setPosition,
  setBranchTarget,
  setPositions,
  setThen,
  thenChoices,
  toGraph,
  uniqueId,
} from "./workflowGraph.mjs";
import { removeBoundaryOn, renameBoundaryOn } from "./forms/boundaryModel.mjs";

const agent = (id, then = []) => ({
  id,
  name: id,
  kind: "agent",
  instructions: `do ${id}`,
  then,
  join: "all",
  on_fail: { on_fail: "fail" },
  retries: 0,
  max_visits: 3,
});

const decide = (id, then = []) => ({
  id,
  name: id,
  kind: "decide",
  rules: [{ when: { condition: "outcome", step: "test", passed: true }, branch: "pass" }],
  otherwise: "fail",
  then,
  join: "all",
  on_fail: { on_fail: "fail" },
  retries: 0,
  max_visits: 3,
});

const diamond = () => ({
  name: "d",
  inputs: [],
  steps: [
    agent("design", [{ to: "build" }, { to: "docs" }]),
    agent("build", [{ to: "ship" }]),
    agent("docs", [{ to: "ship" }]),
    agent("ship"),
  ],
  tags: [],
});

test("the graph is the steps' own flows, and an on_fail route is an edge of its own kind", () => {
  const wf = diamond();
  wf.steps[1].on_fail = { on_fail: "then", step: "design" };
  const g = toGraph(wf);
  assert.deepEqual(g.nodes.map((n) => n.id), ["design", "build", "docs", "ship"]);
  assert.equal(g.edges.filter((e) => e.kind === "then").length, 4);
  const fail = g.edges.find((e) => e.kind === "on_fail");
  assert.deepEqual([fail.from, fail.to], ["build", "design"]);
});

test("the start is the step nothing flows into", () => {
  assert.deepEqual(toGraph(diamond()).nodes.filter((n) => incoming(diamond(), n.id).length === 0).map((n) => n.id), ["design"]);
  assert.deepEqual(incoming(diamond(), "ship").map((i) => i.from).sort(), ["build", "docs"]);
});

test("ids are minted from the kind and never collide", () => {
  const wf = { steps: [agent("agent"), agent("agent-2")] };
  assert.equal(uniqueId(wf, "agent"), "agent-3");
  assert.equal(uniqueId(wf, "human"), "human");
  const { wf: next, id } = addStep(wf, "human");
  assert.equal(id, "human");
  assert.equal(next.steps.length, 3);
  assert.equal(wf.steps.length, 2, "the original is untouched");
});

test("connecting refuses what the validator would, and says why", () => {
  const wf = diamond();
  assert.equal(connect(wf, "ship", "ship").ok, false);
  assert.match(connect(wf, "ship", "ship").reason, /itself/);
  assert.match(connect(wf, "design", "build").reason, /already exists/);
  assert.match(connect(wf, "design", "ship", "yes").reason, /only a branching step/);
  assert.match(connect(wf, "nope", "ship").reason, /both ends/);
  const ended = { ...wf, steps: [...wf.steps, { ...agent("done"), kind: "end" }] };
  assert.match(connect(ended, "done", "ship").reason, /end step/);
  const started = { ...wf, steps: [{ id: "begin", name: "Begin", kind: "start", on: { event: "manual" }, then: [] }, ...wf.steps] };
  assert.match(connect(started, "ship", "begin").reason, /start begins a run/, "nothing flows into a start");
  assert.equal(connect(started, "begin", "design").ok, true, "a start flows out like any step");
  // A cycle is legal — bounded by max_visits — so it is not refused here.
  assert.equal(connect(wf, "ship", "design").ok, true);
});

test("a decide step's flows carry its branches and nothing else", () => {
  const wf = { steps: [decide("verdict"), agent("ship"), agent("fix")] };
  assert.match(connect(wf, "verdict", "ship").reason, /carries a branch/);
  assert.match(connect(wf, "verdict", "ship", "maybe").reason, /no branch maybe/);
  const a = connect(wf, "verdict", "ship", "pass");
  assert.equal(a.ok, true);
  assert.deepEqual(a.wf.steps[0].then, [{ to: "ship", branch: "pass" }]);
  assert.match(connect(a.wf, "verdict", "fix", "pass").reason, /already flows/);
  const b = connect(a.wf, "verdict", "fix", "fail");
  assert.equal(b.ok, true);
  assert.equal(b.wf.steps[0].then.length, 2);
});

test("disconnect touches one flow", () => {
  const wf = { steps: [decide("verdict", [{ to: "a", branch: "pass" }, { to: "b", branch: "fail" }]), agent("a"), agent("b")] };
  const cut = disconnect(wf, "verdict", "a", "pass");
  assert.deepEqual(cut.steps[0].then, [{ to: "b", branch: "fail" }]);
});

test("an on-fail route is set and cleared on one step", () => {
  const wf = { steps: [agent("a"), agent("b")] };
  const routed = setOnFail(wf, "a", { on_fail: "then", step: "b" });
  assert.deepEqual(routed.steps[0].on_fail, { on_fail: "then", step: "b" });
  assert.equal(routed.steps[1], wf.steps[1], "the other step is untouched");
  const cleared = setOnFail(routed, "a", { on_fail: "fail" });
  assert.deepEqual(cleared.steps[0].on_fail, { on_fail: "fail" });
  assert.equal(setOnFail(wf, "zzz", { on_fail: "skip" }), wf, "an unknown step changes nothing");
});

test("removing a step strips the flows into it and falls an on_fail route back to fail", () => {
  const wf = diamond();
  wf.steps[1].on_fail = { on_fail: "then", step: "docs" };
  const next = removeStep(wf, "docs");
  assert.deepEqual(next.steps.map((s) => s.id), ["design", "build", "ship"]);
  assert.deepEqual(next.steps[0].then, [{ to: "build" }]);
  assert.deepEqual(next.steps[1].on_fail, { on_fail: "fail" });
});

test("renaming a step rewrites every place the definition knows it", () => {
  const wf = {
    steps: [
      { ...agent("test", [{ to: "verdict" }]), instructions: "run tests after {steps.build.output.sha}" },
      {
        ...decide("verdict", [{ to: "fix", branch: "fail" }]),
        rules: [{ when: { condition: "outcome", step: "test", passed: true }, branch: "pass" }],
      },
      { ...agent("fix"), on_fail: { on_fail: "then", step: "test" } },
      { id: "schema", name: "s", kind: "check", check: { check: "schema", schema: {}, of: "test" }, then: [] },
      { id: "tell", name: "t", kind: "notify", template: "tests: {steps.test.output}", then: [] },
    ],
  };
  const next = renameStep(wf, "test", "tests");
  assert.equal(next.steps[0].id, "tests");
  assert.equal(next.steps[1].rules[0].when.step, "tests");
  assert.deepEqual(next.steps[2].on_fail, { on_fail: "then", step: "tests" });
  assert.equal(next.steps[3].check.of, "tests");
  assert.equal(next.steps[4].template, "tests: {steps.tests.output}");
  // A flow *into* the renamed step follows it.
  const withFlow = renameStep({ steps: [agent("a", [{ to: "b" }]), agent("b")] }, "b", "c");
  assert.deepEqual(withFlow.steps[0].then, [{ to: "c" }]);
  // Renaming to itself is a no-op returning the same object.
  assert.equal(renameStep(wf, "test", "test"), wf);
});

test("renaming an input rewrites every placeholder and reference that reads it", () => {
  const wf = {
    inputs: [
      { name: "topic", label: "T", kind: "text", required: true },
      { name: "topical", label: "T2", kind: "text", required: false },
    ],
    steps: [
      {
        ...agent("write"),
        instructions: "write on {inputs.topic}, not {inputs.topical}; literal {{inputs.topic}} stays; {{{inputs.topic}}} moves",
        assignee: { input: "topic" },
        project: { input: "topical" },
      },
      { id: "ask", name: "a", kind: "human", prompt: "{inputs.topic}?", assignee: { input: "topic" }, then: [] },
      {
        id: "hold",
        name: "h",
        kind: "wait",
        until: { until: "signal", name: "{inputs.topic}", fields: { env: "{inputs.topic}", other: "x" } },
        then: [],
      },
      { id: "nap", name: "n", kind: "wait", until: { until: "delay", secs: { input: "topic" } }, then: [] },
      { id: "tell", name: "t", kind: "notify", scope: "{inputs.topic}", template: "on {inputs.topic}", mentions: [{ input: "topic" }, { agent: "x" }], author: { input: "topic" }, then: [] },
      { id: "child", name: "c", kind: "spawn", statement_template: "{inputs.topic.deep}", assignees: [{ input: "topic" }], wait: true, then: [] },
      { ...decide("which"), rules: [{ when: { condition: "input_equals", input: "topic", value: "x" }, branch: "yes" }] },
    ],
  };
  const next = renameInput(wf, "topic", "subject");
  assert.deepEqual(next.inputs.map((i) => i.name), ["subject", "topical"]);
  const [write, ask, hold, nap, tell, child, which] = next.steps;
  assert.equal(
    write.instructions,
    "write on {inputs.subject}, not {inputs.topical}; literal {{inputs.topic}} stays; {{{inputs.subject}}} moves",
  );
  assert.deepEqual(write.assignee, { input: "subject" });
  assert.deepEqual(write.project, { input: "topical" }, "another input is untouched");
  assert.equal(ask.prompt, "{inputs.subject}?");
  assert.deepEqual(ask.assignee, { input: "subject" });
  assert.equal(hold.until.name, "{inputs.subject}");
  assert.deepEqual(hold.until.fields, { env: "{inputs.subject}", other: "x" });
  assert.deepEqual(nap.until.secs, { input: "subject" });
  assert.equal(tell.scope, "{inputs.subject}");
  assert.equal(tell.template, "on {inputs.subject}");
  assert.deepEqual(tell.author, { input: "subject" }, "the voice follows the renamed input");
  assert.deepEqual(tell.mentions, [{ input: "subject" }, { agent: "x" }]);
  assert.equal(child.statement_template, "{inputs.subject.deep}");
  assert.deepEqual(child.assignees, [{ input: "subject" }]);
  assert.equal(which.rules[0].when.input, "subject");
  assert.equal(renameInput(wf, "topic", "topic"), wf, "the same name is no edit");
  assert.equal(renameInput(wf, "topic", ""), wf, "an empty name is no edit");
});

test("duplicating copies the shape under a fresh id with no flows", () => {
  const wf = diamond();
  const { wf: next, id } = duplicateStep(wf, "build");
  assert.equal(id, "build-2");
  const copy = next.steps.find((s) => s.id === id);
  assert.equal(copy.name, "build (copy)");
  assert.deepEqual(copy.then, []);
  assert.equal(copy.instructions, "do build");
  assert.equal(duplicateStep(wf, "nope").id, null);
});

test("a step's position is its own: set one, set several in one edit, dropped with a position, and a duplicate lands offset", () => {
  const wf = diamond();
  const one = setPosition(wf, "build", { x: 40, y: 80 });
  assert.notEqual(one, wf);
  assert.deepEqual(one.steps.find((s) => s.id === "build").position, { x: 40, y: 80 });
  assert.equal(one.steps.find((s) => s.id === "ship").position, undefined, "the others are untouched");
  const many = setPositions(wf, new Map([["design", { x: 0, y: 0 }], ["ship", { x: 0, y: 300 }]]));
  assert.deepEqual(many.steps.filter((s) => s.position).map((s) => s.id), ["design", "ship"]);
  assert.equal(setPositions(wf, new Map()), wf, "nothing to set: the same object");
  const dropped = addStep(wf, "human", undefined, { x: 8, y: 16 });
  assert.deepEqual(dropped.wf.steps.at(-1).position, { x: 8, y: 16 });
  assert.equal(addStep(wf, "human").wf.steps.at(-1).position, undefined, "a click adds a placeless step");
  const { wf: copied, id } = duplicateStep(one, "build");
  assert.deepEqual(copied.steps.find((s) => s.id === id).position, { x: 40 + DUPLICATE_OFFSET, y: 80 + DUPLICATE_OFFSET });
  assert.equal(duplicateStep(wf, "build").wf.steps.at(-1).position, undefined, "a placeless source makes a placeless copy");
  const renamed = renameStep(one, "build", "make");
  assert.deepEqual(renamed.steps.find((s) => s.id === "make").position, { x: 40, y: 80 }, "the position rides with the step");
});

test("replacing a step keeps its position", () => {
  const wf = diamond();
  const next = replaceStep(wf, "build", { ...agent("build", [{ to: "ship" }]), name: "Build it" });
  assert.equal(next.steps[1].name, "Build it");
  assert.equal(next.steps.length, 4);
});

test("every canvas edit passes one gate", () => {
  assert.equal(mayEdit({ readOnly: false, editable: null }, "a"), true);
  assert.equal(mayEdit({ readOnly: true, editable: null }, "a"), false, "read-only refuses everything");
  const started = { readOnly: false, editable: new Set(["b"]) };
  assert.equal(mayEdit(started, "a"), false, "a started step is frozen in an amendment");
  assert.equal(mayEdit(started, "b"), true);
  assert.equal(mayEdit(started, null), true, "a drop names no step and is judged by the gate alone");
  assert.equal(mayEdit({ readOnly: true, editable: new Set(["b"]) }, "b"), false);
  assert.equal(mayEdit(null, "a"), true, "no gate means every step");
});

test("every branching kind connects by its branches and refuses an unlabelled or foreign one", () => {
  const cond = { condition: "between", from_hour: 1, to_hour: 2 };
  const wf = {
    steps: [
      { ...agent("i"), kind: "if", when: cond },
      { ...agent("s"), kind: "switch", on: "{inputs.env}", cases: [{ value: "prod", branch: "live" }], otherwise: "rest" },
      { ...agent("j"), kind: "judge", state: "{inputs.env}", instructions: "which?", options: [{ branch: "yes", meaning: "sure" }], otherwise: "unsure" },
      { ...agent("f"), kind: "for_each", items: "{inputs.list}", max_iterations: 10 },
      { ...agent("w"), kind: "while", when: cond, max_iterations: 10 },
      agent("a"),
    ],
  };
  assert.equal(connect(wf, "i", "a", "yes").ok, true);
  assert.equal(connect(wf, "i", "a", "maybe").ok, false, "an if has yes and no only");
  assert.equal(connect(wf, "i", "a").ok, false, "an if labels its flows");
  assert.equal(connect(wf, "s", "a", "live").ok, true);
  assert.equal(connect(wf, "s", "a", "rest").ok, true);
  assert.equal(connect(wf, "s", "a", "nope").ok, false);
  assert.equal(connect(wf, "j", "a", "yes").ok, true);
  assert.equal(connect(wf, "j", "a", "unsure").ok, true);
  assert.equal(connect(wf, "j", "a", "nope").ok, false, "a judge step's branches are its options and otherwise");
  assert.equal(connect(wf, "f", "a", "each").ok, true);
  assert.equal(connect(wf, "f", "a", "done").ok, true);
  assert.equal(connect(wf, "w", "a", "loop").ok, true);
  assert.equal(connect(wf, "a", "i", "yes").ok, false, "a plain step does not label");
  // The body's way back into the loop is a plain flow.
  const body = connect(wf, "a", "f");
  assert.equal(body.ok, true);
  const g = toGraph(connect(body.wf, "f", "a", "each").wf);
  assert.ok(g.edges.find((e) => e.id === "a->f").loop, "the flow back into the loop step is the loop edge");
  assert.ok(!g.edges.find((e) => e.id === "f->a#each").loop);
  for (const id of ["i", "s", "j", "f", "w"]) assert.ok(isBranching(wf.steps.find((x) => x.id === id)));
});

test("a switch case is relabelled with its flows, and an if or a loop refuses a rename", () => {
  const sw = {
    id: "s",
    name: "S",
    kind: "switch",
    on: "{inputs.env}",
    cases: [{ value: "prod", branch: "live" }],
    otherwise: "rest",
    then: [
      { to: "a", branch: "live" },
      { to: "b", branch: "rest" },
    ],
  };
  const r = relabelBranch(sw, "live", "production");
  assert.equal(r.ok, true);
  assert.equal(r.step.cases[0].branch, "production");
  assert.deepEqual(r.step.then[0], { to: "a", branch: "production" });
  const o = relabelBranch(sw, "rest", "other");
  assert.equal(o.step.otherwise, "other");
  assert.equal(relabelBranch(sw, "live", "rest").ok, false, "a name another branch has");
  assert.equal(relabelBranch({ id: "i", kind: "if", when: {} }, "yes", "ja").ok, false, "fixed words");
  assert.equal(relabelBranch({ id: "f", kind: "for_each", items: "" }, "each", "item").ok, false);
});

test("a judge option is relabelled with its flows, like a switch case", () => {
  const j = {
    id: "j",
    name: "J",
    kind: "judge",
    state: "{inputs.env}",
    instructions: "sure?",
    options: [{ branch: "urgent", meaning: "needs a person today" }],
    otherwise: "unsure",
    then: [
      { to: "a", branch: "urgent" },
      { to: "b", branch: "unsure" },
    ],
  };
  const r = relabelBranch(j, "urgent", "today");
  assert.equal(r.ok, true);
  assert.equal(r.step.options[0].branch, "today");
  assert.deepEqual(r.step.then[0], { to: "a", branch: "today" });
  const o = relabelBranch(j, "unsure", "unclear");
  assert.equal(o.step.otherwise, "unclear");
  assert.equal(relabelBranch(j, "urgent", "unsure").ok, false, "a name another branch has");
});

test("renaming a step or an input walks nested conditions and the new kinds' templates", () => {
  const nested = { condition: "all", of: [{ condition: "not", of: { condition: "outcome", step: "test", passed: true } }, { condition: "input_equals", input: "env", value: "x" }] };
  const wf = {
    inputs: [{ name: "env", label: "E", kind: "text", required: true }, { name: "list", label: "L", kind: "text", required: true }],
    steps: [
      agent("test"),
      { ...agent("i"), kind: "if", when: nested },
      { ...agent("w"), kind: "while", when: nested, max_iterations: 3 },
      { ...agent("s"), kind: "switch", on: "{inputs.env} {steps.test.output.x}", cases: [], otherwise: "o" },
      {
        ...agent("j"),
        kind: "judge",
        state: "{inputs.env} {steps.test.output.x}",
        instructions: "consider {steps.test.output.x}",
        options: [],
        otherwise: "o",
      },
      { ...agent("f"), kind: "for_each", items: "{steps.test.output.items}", max_iterations: 3 },
      { ...agent("c"), kind: "connector", connector: "slack", operation: "post", account: { input: "env" }, params: { text: "{inputs.env} {steps.test.output.x}" } },
    ],
  };
  const byStep = renameStep(wf, "test", "tests");
  assert.equal(byStep.steps[1].when.of[0].of.step, "tests");
  assert.equal(byStep.steps[2].when.of[0].of.step, "tests");
  assert.equal(byStep.steps[3].on, "{inputs.env} {steps.tests.output.x}");
  assert.equal(byStep.steps[4].state, "{inputs.env} {steps.tests.output.x}");
  assert.equal(byStep.steps[4].instructions, "consider {steps.tests.output.x}");
  assert.equal(byStep.steps[5].items, "{steps.tests.output.items}");
  assert.equal(byStep.steps[6].params.text, "{inputs.env} {steps.tests.output.x}");
  const byInput = renameInput(wf, "env", "environment");
  assert.equal(byInput.steps[1].when.of[1].input, "environment");
  assert.equal(byInput.steps[3].on, "{inputs.environment} {steps.test.output.x}");
  assert.equal(byInput.steps[4].state, "{inputs.environment} {steps.test.output.x}");
  assert.deepEqual(byInput.steps[6].account, { input: "environment" });
  assert.equal(byInput.steps[6].params.text, "{inputs.environment} {steps.test.output.x}");
});

test("renaming a branch carries its rule, otherwise and flows along, and refuses a clash", () => {
  const step = {
    id: "d",
    name: "D",
    kind: "decide",
    rules: [{ when: { condition: "between", from_hour: 1, to_hour: 2 }, branch: "day" }],
    otherwise: "night",
    then: [
      { to: "a", branch: "day" },
      { to: "b", branch: "night" },
    ],
  };
  const r = relabelBranch(step, "day", "office");
  assert.equal(r.ok, true);
  assert.equal(r.step.rules[0].branch, "office");
  assert.equal(r.step.otherwise, "night");
  assert.deepEqual(r.step.then, [
    { to: "a", branch: "office" },
    { to: "b", branch: "night" },
  ]);
  const o = relabelBranch(step, "night", "home");
  assert.equal(o.step.otherwise, "home");
  assert.equal(o.step.then[1].branch, "home");
  assert.equal(relabelBranch(step, "day", "night").ok, false, "a name another branch has");
  assert.equal(relabelBranch(step, "day", "  ").ok, false, "a blank name");
  assert.equal(relabelBranch(step, "day", "day").step, step, "the same name is no edit");
  assert.equal(relabelBranch({ id: "a", kind: "agent" }, "x", "y").ok, false);
});

/** An approval with a timeout that diverts to an escalation and a reminder posted beside it. */
const guarded = () => ({
  name: "g",
  inputs: [{ name: "patience", label: "P", kind: "number", required: false, default: 3600 }],
  steps: [
    { id: "begin", name: "Begin", kind: "start", on: { event: "manual" }, then: [{ to: "review" }] },
    {
      id: "review",
      name: "Review",
      kind: "approval",
      prompt: "Ship it?",
      then: [{ to: "ship" }, { to: "escalate", branch: "late" }],
      boundaries: [
        { name: "late", on: { event: "after", secs: { input: "patience" } }, act: "divert" },
        { name: "nudge", on: { event: "every", secs: 86400, max: 5 }, act: "notify", template: "Still waiting on {steps.begin.output}", author: { input: "patience" } },
      ],
    },
    agent("ship"),
    agent("escalate"),
  ],
  tags: [],
});

test("a flow labelled with a divert is a boundary edge; the step's own flow stays a flow", () => {
  const g = toGraph(guarded());
  const late = g.edges.find((e) => e.id === "review->escalate#late");
  assert.equal(late.kind, "boundary");
  assert.equal(late.branch, "late");
  assert.equal(g.edges.find((e) => e.id === "review->ship").kind, "then");
  assert.equal(g.edges.find((e) => e.id === "begin->review").kind, "then");
  assert.ok(!g.edges.some((e) => e.loop), "a start is a root, and nothing loops");
  // An act has no path: a flow labelled with a reminder's name is no boundary edge.
  const odd = guarded();
  odd.steps[1].then.push({ to: "ship", branch: "nudge" });
  assert.equal(toGraph(odd).edges.find((e) => e.branch === "nudge").kind, "then", "the validator's to refuse, drawn as the flow it is");
});

test("a divert's path is drawn from its chip: one flow per divert, and a label no divert has is refused", () => {
  const cut = disconnect(guarded(), "review", "escalate", "late");
  assert.deepEqual(cut.steps[1].then, [{ to: "ship" }], "cutting the path leaves the boundary");
  assert.equal(cut.steps[1].boundaries.length, 2);
  const redrawn = connect(cut, "review", "escalate", "late");
  assert.equal(redrawn.ok, true);
  assert.deepEqual(redrawn.wf.steps[1].then, [{ to: "ship" }, { to: "escalate", branch: "late" }]);
  assert.match(connect(guarded(), "review", "ship", "late").reason, /already flows/, "one path per divert");
  assert.match(connect(guarded(), "review", "ship", "nudge").reason, /only a branching step/, "a reminder posts; it has no path");
  assert.match(connect(guarded(), "review", "ship", "never").reason, /only a branching step/);
});

test("a boundary renamed carries its path; removed, its path goes with it; a removed target takes the path too", () => {
  // The inspector's road: the step's own edit (`boundaryModel`), written back whole (`replaceStep`).
  const wf = guarded();
  const review = wf.steps[1];
  const renamed = renameBoundaryOn(review, "late", "overdue");
  assert.equal(renamed.ok, true);
  const after = replaceStep(wf, "review", renamed.step);
  assert.deepEqual(after.steps[1].then, [{ to: "ship" }, { to: "escalate", branch: "overdue" }]);
  assert.deepEqual(after.steps[1].boundaries.map((b) => b.name), ["overdue", "nudge"]);
  assert.deepEqual(toGraph(after).edges.filter((e) => e.kind === "boundary").map((e) => [e.to, e.branch]), [["escalate", "overdue"]], "the canvas draws the path under its new name");
  assert.equal(renameBoundaryOn(review, "late", "nudge").ok, false, "a name the step uses");
  const removed = replaceStep(wf, "review", removeBoundaryOn(review, "late"));
  assert.deepEqual(removed.steps[1].then, [{ to: "ship" }]);
  assert.deepEqual(removed.steps[1].boundaries.map((b) => b.name), ["nudge"]);
  const gone = removeStep(guarded(), "escalate");
  assert.deepEqual(gone.steps[1].then, [{ to: "ship" }], "the divert's path to a removed step goes with it");
  assert.equal(gone.steps[1].boundaries.length, 2, "the boundary stays for the validator to ask a path of");
});

test("a failure is routed to a step something may flow into: never the step itself, never a start, never a blank reference", () => {
  const begin = { id: "begin", name: "By hand", kind: "start", on: { event: "manual" }, then: [{ to: "build" }] };
  const nightly = { id: "nightly", name: "Nightly", kind: "start", on: { event: "schedule", every: 3600 }, then: [{ to: "build" }] };
  const wf = { name: "w", steps: [begin, nightly, agent("build", [{ to: "test" }]), agent("test"), agent("fix", [{ to: "build" }])] };
  assert.deepEqual(failTargets(wf.steps, "test").map((s) => s.id), ["build", "fix"], "a start begins a run: nothing flows into it, a failure neither");
  assert.deepEqual(failTargets(wf.steps, "build").map((s) => s.id), ["test", "fix"]);
  assert.deepEqual(failTargets(null, "x"), []);

  const test_ = wf.steps[3];
  assert.deepEqual(failChoice(wf.steps, test_, "then"), { on_fail: "then", step: "build" }, "the first step a failure may reach — not the start that leads the list");
  assert.deepEqual(failChoice(wf.steps, { ...test_, on_fail: { on_fail: "then", step: "fix" } }, "then"), { on_fail: "then", step: "fix" }, "a route already chosen is kept");
  assert.deepEqual(failChoice(wf.steps, { ...test_, on_fail: { on_fail: "then", step: "begin" } }, "then"), { on_fail: "then", step: "build" }, "one that leads into a start is moved off it");
  assert.deepEqual(failChoice(wf.steps, test_, "skip"), { on_fail: "skip" });
  assert.deepEqual(failChoice(wf.steps, test_, "fail"), { on_fail: "fail" });
  assert.deepEqual(failChoice(wf.steps, test_, "nonsense"), { on_fail: "fail" }, "a word that is none is the default");
  // A new workflow — a start and one step: there is nowhere to route a failure, and no `""` is written for the node to refuse.
  const young = [begin, agent("build")];
  assert.deepEqual(failTargets(young, "build"), []);
  assert.equal(failChoice(young, young[1], "then"), null, "refused: the step keeps what it has");
  const form = readFileSync(new URL("./forms/StepCommonForm.tsx", import.meta.url), "utf8");
  assert.ok(form.includes("failChoice(steps, step, e.target.value)") && form.includes("failTargets(steps, step.id)"), "the form asks the model");
  assert.equal(/\?\? ""/.test(form), false, "no blank reference is born in the form");
});

test("renaming a step or an input reaches starts, catches, emits and boundary events", () => {
  const wf = guarded();
  wf.inputs.push({ name: "channel", label: "C", kind: "text", required: true }, { name: "who", label: "W", kind: "assignee", required: true });
  wf.steps.push(
    { id: "ticket", name: "T", kind: "start", on: { event: "message", in: "{inputs.channel}", from: { input: "who" } }, inputs: { channel: "{event.payload.scope}" }, then: [{ to: "review" }] },
    { id: "nightly", name: "N", kind: "start", on: { event: "schedule", every: { input: "patience" } }, then: [{ to: "review" }] },
    { id: "hold", name: "H", kind: "wait", until: { until: "message", in: "{inputs.channel}", from: { input: "who" }, contains: "done" }, then: [] },
    { id: "tell", name: "E", kind: "emit", signal: "report.ready", payload: { url: "{steps.ship.output.url}", channel: "{inputs.channel}" }, then: [] },
  );
  const byInput = renameInput(renameInput(wf, "channel", "room"), "patience", "grace");
  const step = (id) => byInput.steps.find((s) => s.id === id);
  assert.equal(step("ticket").on.in, "{inputs.room}", "a start's event field reads the inputs it listens with");
  assert.deepEqual(step("ticket").inputs, { room: "{event.payload.scope}" }, "the mapping is keyed by the input it fills");
  assert.deepEqual(step("nightly").on.every, { input: "grace" }, "a cadence read from an input");
  assert.equal(step("hold").until.in, "{inputs.room}");
  assert.equal(step("tell").payload.channel, "{inputs.room}");
  assert.deepEqual(step("review").boundaries[0].on.secs, { input: "grace" }, "a timeout's clock");
  assert.deepEqual(step("review").boundaries[1].author, { input: "grace" }, "a reminder's voice read from an input follows it");
  assert.deepEqual(renameInput(wf, "channel", "room").steps.find((s) => s.id === "review").boundaries[1].author, { input: "patience" }, "an input of another name is untouched");
  const byWho = renameInput(wf, "who", "someone");
  assert.deepEqual(byWho.steps.find((s) => s.id === "ticket").on.from, { input: "someone" }, "who wrote the message");
  assert.deepEqual(byWho.steps.find((s) => s.id === "hold").until.from, { input: "someone" });
  const byStep = renameStep(wf, "ship", "deliver");
  assert.equal(byStep.steps.find((s) => s.id === "tell").payload.url, "{steps.deliver.output.url}", "an emit's payload reads the run");
  const byBegin = renameStep(wf, "begin", "open");
  assert.equal(byBegin.steps.find((s) => s.id === "review").boundaries[1].template, "Still waiting on {steps.open.output}", "a reminder's post reads the run");
  assert.deepEqual(byBegin.steps.find((s) => s.id === "open").then, [{ to: "review" }]);
});

test("a new rule, case or option takes a branch name its step has for nothing else — one rule for the three gateways' forms", () => {
  const decide = { id: "d", kind: "decide", rules: [{ when: { condition: "all", of: [] }, branch: "branch-1" }], otherwise: "otherwise", then: [] };
  assert.equal(freshBranch(decide), "branch-2");
  assert.equal(freshBranch({ ...decide, rules: [], otherwise: "branch-1" }), "branch-2", "`otherwise` holds a name too");
  assert.equal(freshBranch({ ...decide, rules: [{ when: {}, branch: "branch-2" }] }), "branch-1", "the first that is free");
  const sw = { id: "s", kind: "switch", on: "{inputs.x}", cases: [{ value: "a", branch: "case-1" }], otherwise: "otherwise", then: [] };
  assert.equal(freshBranch(sw), "case-2", "a switch's branches are its cases'");
  const judge = { id: "j", kind: "judge", state: "", instructions: "", options: [{ branch: "branch-1", meaning: "" }, { branch: "branch-2", meaning: "" }], otherwise: "otherwise", then: [] };
  assert.equal(freshBranch(judge), "branch-3");
  // A step that carries a divert: its boundary's name is a label its flows carry, so no branch takes it.
  assert.equal(freshBranch({ ...decide, boundaries: [{ name: "branch-2", on: { event: "after", secs: 60 }, act: "divert" }] }), "branch-3");
  for (const form of ["DecideStepForm", "SwitchStepForm", "JudgeStepForm"]) {
    const text = readFileSync(new URL(`./forms/${form}.tsx`, import.meta.url), "utf8");
    // The judge's form adds an option through its model, which asks the same rule.
    const asks = form === "JudgeStepForm" ? readFileSync(new URL("./forms/judgeStepModel.mjs", import.meta.url), "utf8") : text;
    assert.ok(text.includes('from "./BranchName"') && asks.includes("freshBranch(step)"), `${form} draws the one field and asks the one rule`);
    assert.equal(/function (BranchName|freshBranch)\(/.test(text), false, `${form} keeps no copy of either`);
  }
});

test("a step's upstream is every step a flow leads from, however far — in the definition's order, and never the step itself", () => {
  const wf = diamond();
  assert.deepEqual(upstreamOf(wf, "ship").map((s) => s.id), ["design", "build", "docs"]);
  assert.deepEqual(upstreamOf(wf, "build").map((s) => s.id), ["design"]);
  assert.deepEqual(upstreamOf(wf, "design"), [], "nothing flows into the first step");
  assert.deepEqual(upstreamOf(wf, "nowhere"), []);
  // A rework loop: ship flows back to build. Each sees the other, neither sees itself.
  wf.steps[3].then = [{ to: "build" }];
  assert.deepEqual(upstreamOf(wf, "build").map((s) => s.id), ["design", "docs", "ship"]);
  assert.deepEqual(upstreamOf(wf, "ship").map((s) => s.id), ["design", "build", "docs"]);
  // A chain long enough that a walk which re-reads the whole graph at every step would be felt.
  const long = { steps: Array.from({ length: 400 }, (_, i) => agent(`s${i}`, i < 399 ? [{ to: `s${i + 1}` }] : [])) };
  assert.equal(upstreamOf(long, "s399").length, 399);
  assert.deepEqual(upstreamOf(null, "a"), []);
  const inspector = readFileSync(new URL("./Inspector.tsx", import.meta.url), "utf8");
  assert.ok(inspector.includes("upstreamOf(wf, step.id)") && !inspector.includes("function upstreamOf"), "the inspector asks the model");
});

test("Then, from the keyboard: a plain step's targets with the flows it has, a gateway's branches with where each goes — every change through connect", () => {
  const started = { steps: [{ id: "begin", name: "Begin", kind: "start", on: { event: "manual" }, then: [{ to: "design" }] }, ...diamond().steps] };
  const plain = thenChoices(started, "design");
  assert.equal(plain.branching, false);
  assert.deepEqual(plain.targets.map((x) => [x.id, x.on]), [["build", true], ["docs", true], ["ship", false]], "never itself, never a start");
  assert.deepEqual(thenChoices({ steps: [{ ...agent("done"), kind: "end" }, agent("x")] }, "done").targets, [], "an end has nothing after");
  assert.deepEqual(thenChoices(started, "nowhere").targets, []);

  const on = setThen(started, "design", "ship", true);
  assert.equal(on.ok, true);
  assert.deepEqual(on.wf.steps[1].then.map((f) => f.to), ["build", "docs", "ship"]);
  const off = setThen(on.wf, "design", "build", false);
  assert.deepEqual(off.wf.steps[1].then.map((f) => f.to), ["docs", "ship"]);
  assert.match(setThen(started, "ship", "begin", true).reason, /start begins a run/, "the canvas's refusals hold");

  const gate = { steps: [decide("verdict", [{ to: "ship", branch: "pass" }]), agent("ship"), agent("fix")] };
  const g = thenChoices(gate, "verdict");
  assert.equal(g.branching, true);
  assert.deepEqual(g.branches, [{ branch: "pass", to: "ship" }, { branch: "fail", to: null }]);
  const moved = setBranchTarget(gate, "verdict", "pass", "fix");
  assert.deepEqual(moved.wf.steps[0].then, [{ to: "fix", branch: "pass" }], "the old flow of the branch is cut");
  const added = setBranchTarget(moved.wf, "verdict", "fail", "ship");
  assert.deepEqual(added.wf.steps[0].then, [{ to: "fix", branch: "pass" }, { to: "ship", branch: "fail" }]);
  const cleared = setBranchTarget(added.wf, "verdict", "pass", null);
  assert.deepEqual(cleared.wf.steps[0].then, [{ to: "ship", branch: "fail" }]);
  assert.equal(setBranchTarget(gate, "verdict", "pass", "ship").wf, gate, "the same pick is no edit");
  assert.equal(setBranchTarget(gate, "verdict", "pass", "verdict").ok, false, "a refused pick leaves the old flow standing");
});

// added by the coverage pass: workflowGraph.test.mjs
test("renaming an input follows it into a poll's parameters, a check's command and an emit boundary", () => {
  const wf = {
    inputs: [{ name: "topic", label: "T", kind: "text" }],
    steps: [
      { id: "poll", name: "p", kind: "start", on: { event: "connector", connector: "chat", operation: "list", params: { channel: "{inputs.topic}" }, every: 60 }, inputs: {}, then: ["work"] },
      { id: "watch", name: "w", kind: "start", on: { event: "check", command: "test -f {inputs.topic}", every: 60 }, inputs: {}, then: ["work"] },
      {
        id: "work",
        name: "w",
        kind: "agent",
        instructions: "do it",
        boundaries: [{ name: "late", on: { event: "after", secs: 60 }, act: "emit", signal: "{inputs.topic}.late", payload: { about: "{inputs.topic}" } }],
        then: [],
      },
    ],
  };
  const next = renameInput(wf, "topic", "subject");
  const [poll, watch, work] = next.steps;
  assert.equal(poll.on.params.channel, "{inputs.subject}");
  assert.equal(watch.on.command, "test -f {inputs.subject}");
  assert.equal(work.boundaries[0].signal, "{inputs.subject}.late");
  assert.deepEqual(work.boundaries[0].payload, { about: "{inputs.subject}" });
});

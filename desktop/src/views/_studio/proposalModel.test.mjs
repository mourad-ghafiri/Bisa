import test from "node:test";
import assert from "node:assert/strict";
import { adoptionInputs, changeRequest, flowSentence, inputsNeeded, listens, loopSentences, proposalHeadline, startsOn, stepLines } from "./proposalModel.mjs";
import { tx } from "../../i18n/l10n.mjs";

const proposal = {
  workflow: "W1",
  revision: 2,
  name: "Ship it",
  description: "Build, review, ship.",
  inputs: [
    { name: "repo", label: "Repository", required: true },
    { name: "tone", label: "Tone", required: true, default: "warm" },
    { name: "notes", required: false },
  ],
  steps: [
    { id: "plan", name: "Plan", kind: "agent", summary: { id: "step-summary-agent-assigned", args: { what: "Draft the plan", who: "agent:dev" } }, assignee: "agent:dev", join_any: false, max_visits: 3 },
    { id: "gate", name: "Decide", kind: "decide", summary: { id: "step-summary-decide", args: { n: 1, branches: "ok", otherwise: "redo" } }, join_any: false, max_visits: 3 },
    { id: "ship", name: "", kind: "check", summary: { id: "step-summary-check-command", args: { command: "deploy" } }, join_any: true, on_fail: "then fix", max_visits: 5 },
    { id: "fix", name: "Fix", kind: "agent", summary: { id: "step-summary-agent", args: { what: "Fix it" } }, max_visits: 3 },
  ],
  edges: [
    { from: "plan", to: "gate", kind: "then" },
    { from: "gate", to: "ship", branch: "ok", kind: "then" },
    { from: "gate", to: "plan", branch: "redo", kind: "then" },
    { from: "ship", to: "fix", kind: "on_fail" },
    { from: "fix", to: "ship", kind: "then" },
  ],
};

test("the headline names the workflow, the count and the revision", () => {
  assert.equal(proposalHeadline(proposal), "Ship it · 4 steps · rev 2");
  assert.equal(proposalHeadline({ name: "One", steps: [{}], revision: 1 }), "One · 1 step · rev 1");
  assert.equal(proposalHeadline(null), "Untitled workflow · 0 steps · rev 1");
});

test("the flow sentence walks the steps by name, falling back to the id", () => {
  assert.equal(flowSentence(proposal), "Plan → Decide → ship → Fix");
});

test("every step is a numbered line with its summary, its assignee and what departs from the defaults", () => {
  const lines = stepLines(proposal);
  assert.equal(lines.length, 4);
  assert.deepEqual(lines[0], { index: 1, id: "plan", name: "Plan", kind: "agent", summary: { id: "step-summary-agent-assigned", args: { what: "Draft the plan", who: "agent:dev" } }, assignee: "agent:dev", notes: [] });
  assert.equal(stepLines({ steps: [{ id: "x", name: "X", kind: "agent" }] })[0].summary, null, "a step with no summary has none, not an empty sentence");
  assert.deepEqual(lines[1].notes, ["branches: ok → ship, redo → plan"]);
  assert.deepEqual(lines[2].notes, ["on failure: then fix", "continues when the first flow arrives", "up to 5 visits"]);
  assert.equal(lines[2].name, "ship", "an unnamed step is named by its id");
  assert.equal(lines[3].assignee, null);
});

test("loops are the flows back to an earlier step, said in words", () => {
  assert.deepEqual(loopSentences(proposal), ["gate loops back to plan on redo", "fix loops back to ship"]);
  assert.deepEqual(loopSentences({ steps: [{ id: "a" }, { id: "b" }], edges: [{ from: "a", to: "b", kind: "then" }] }), []);
});

test("the Adopt card lists what the design starts on, in the node's words", () => {
  assert.deepEqual(startsOn(proposal), [], "a design with no start step names none");
  const listening = {
    ...proposal,
    steps: [
      { id: "by-hand", name: "By hand", kind: "start", summary: { id: "step-summary-start-manual" } },
      { id: "weekly", name: "Weekly", kind: "start", summary: { id: "step-summary-start-cron", args: { cron: "0 9 * * 1" } } },
      ...proposal.steps,
    ],
  };
  assert.deepEqual(startsOn(listening).map(tx), ["begins by hand", "begins on the schedule `0 9 * * 1`"]);
  assert.deepEqual(startsOn(null), []);
});

test("the inputs a run needs are the required ones without a default", () => {
  assert.deepEqual(inputsNeeded(proposal), { total: 3, required: ["Repository"] });
  assert.deepEqual(inputsNeeded({}), { total: 0, required: [] });
});

test("a design that begins on an event is adopted by listening, and asks only what listening needs", () => {
  assert.equal(listens(proposal), false, "no start step: it runs");
  assert.deepEqual(adoptionInputs(proposal).map((i) => i.name), ["repo", "tone", "notes"], "a design that runs asks its inputs");
  const byHand = { ...proposal, steps: [{ id: "start", name: "Start", kind: "start", event: "manual" }, ...proposal.steps] };
  assert.equal(listens(byHand), false, "a start by hand is no event");
  assert.equal(listens({ steps: [{ id: "start", kind: "start", event: null }] }), false);

  const standing = {
    ...proposal,
    // The hook maps `repo` from its body; the schedule reads `tone`'s default; nothing fills `notes`.
    listening_needs: ["notes"],
    steps: [
      { id: "start", name: "Start", kind: "start", event: "manual" },
      { id: "ticket", name: "A ticket arrives", kind: "start", event: "hook" },
      ...proposal.steps,
    ],
  };
  assert.equal(listens(standing), true);
  assert.deepEqual(adoptionInputs(standing).map((i) => i.name), ["notes"], "what an event supplies is not asked");
  assert.deepEqual(inputsNeeded(standing), { total: 1, required: ["notes"] }, "what listening asks must be filled");
  assert.deepEqual(adoptionInputs({ ...standing, listening_needs: [] }), [], "a design its events fill asks nothing");
  assert.deepEqual(adoptionInputs({ ...standing, listening_needs: undefined }), []);
  assert.deepEqual(adoptionInputs(null), []);
  assert.equal(listens(null), false);
});

test("a change request is words or nothing", () => {
  assert.equal(changeRequest("  add a review step  "), "add a review step");
  assert.equal(changeRequest("   "), null);
  assert.equal(changeRequest(null), null);
});

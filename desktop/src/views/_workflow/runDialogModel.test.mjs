/**
 * *Run…* on a library workflow, as its dialog decides it: where the run
 * begins, what it asks, what it sends, and how a refusal reads. Run with
 * `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { BY_HAND, askedInputs, entryStart, firstEntry, payloadOf, runEntries, runRefusal, runRequest, sampleText } from "./runDialogModel.mjs";

const input = (name, over = {}) => ({ name, label: name, kind: "text", required: true, ...over });
const byHand = { id: "by-hand", name: "By hand", kind: "start", on: { event: "manual" }, then: [{ to: "draft" }] };
const onHook = { id: "ticket", name: "A ticket arrives", kind: "start", on: { event: "hook", public: false }, inputs: { body: "{event.payload.body}" }, then: [{ to: "draft" }] };
const onSchedule = { id: "weekly", name: "", kind: "start", on: { event: "schedule", cron: "0 9 * * 1", tz: null }, then: [{ to: "draft" }] };
const draft = { id: "draft", name: "Draft", kind: "agent", instructions: "{inputs.body} for {inputs.audience}", then: [] };
const workflow = (starts, inputs = [input("body"), input("audience")]) => ({ id: "wf", name: "Triage", inputs, steps: [...starts, draft] });

test("the dialog offers the ways in: by hand when the workflow has one, then each event start as a test", () => {
  const both = workflow([byHand, onHook, onSchedule]);
  assert.deepEqual(
    runEntries(both).map((e) => [e.id, e.test, e.label]),
    [
      [BY_HAND, false, "By hand"],
      ["ticket", true, "Test: as if “A ticket arrives” happened"],
      ["weekly", true, "Test: as if “weekly” happened"],
    ],
    "a start with no name is said by its id",
  );
  assert.equal(runEntries(both)[1].hint, "when called");
  assert.equal(runEntries(both)[0].hint, null);
  assert.equal(firstEntry(both), BY_HAND, "by hand when it can begin so");
  // Only events begin it: the tests alone, the first one chosen.
  const eventOnly = workflow([onHook, onSchedule]);
  assert.deepEqual(runEntries(eventOnly).map((e) => e.id), ["ticket", "weekly"]);
  assert.equal(firstEntry(eventOnly), "ticket");
  // A workflow that names no start begins by hand at its root: one entry, nothing to choose.
  const plain = workflow([]);
  assert.deepEqual(runEntries(plain).map((e) => e.id), [BY_HAND]);
  assert.equal(firstEntry(plain), BY_HAND);
  assert.equal(firstEntry({ id: "wf", name: "Empty", inputs: [], steps: [] }), null, "nothing to begin at: nothing to run");
});

test("an entry names its start: by hand none, a test its event start — and never a step that is no start of it", () => {
  const both = workflow([byHand, onHook]);
  assert.equal(entryStart(both, BY_HAND), null);
  assert.equal(entryStart(both, "ticket")?.id, "ticket");
  assert.equal(entryStart(both, "draft"), null, "an agent step begins nothing");
  assert.equal(entryStart(both, "by-hand"), null, "the start by hand is no test");
  assert.equal(entryStart(both, "gone"), null);
});

test("a run by hand asks every input; a test asks the ones its start's mapping does not fill from the event", () => {
  const both = workflow([byHand, onHook, onSchedule]);
  assert.deepEqual(askedInputs(both, BY_HAND), { asked: ["body", "audience"], mapped: 0 });
  assert.deepEqual(askedInputs(both, "ticket"), { asked: ["audience"], mapped: 1 }, "the hook's mapping fills `body`");
  assert.deepEqual(askedInputs(both, "weekly"), { asked: ["body", "audience"], mapped: 0 }, "a schedule maps nothing");
  assert.deepEqual(askedInputs(workflow([byHand], []), BY_HAND), { asked: [], mapped: 0 });
});

test("a test's sample is what its event carries, as JSON a person edits; by hand there is none", () => {
  const both = workflow([byHand, onHook, onSchedule]);
  assert.equal(sampleText(both, BY_HAND, 1000), "");
  assert.equal(sampleText(both, "ticket", 1000), "{}");
  assert.equal(sampleText(both, "weekly", 1000), JSON.stringify({ at: 1000 }, null, 2));
});

test("the payload is JSON or a sentence saying why not; an empty box is an empty occurrence", () => {
  assert.deepEqual(payloadOf('{ "body": "It broke" }'), { ok: true, event: { body: "It broke" } });
  assert.deepEqual(payloadOf("   "), { ok: true, event: {} });
  assert.deepEqual(payloadOf(""), { ok: true, event: {} });
  const bad = payloadOf("{ body: nope }");
  assert.equal(bad.ok, false);
  assert.match(bad.error, /^Not JSON: /);
});

test("what the dialog sends: the inputs by hand; the asked inputs, the start and the event for a test; nothing while something is wrong", () => {
  const both = workflow([byHand, onHook]);
  assert.deepEqual(runRequest(both, BY_HAND, { body: " It broke ", audience: "support" }, ""), { kind: "run", inputs: { body: "It broke", audience: "support" } });
  assert.deepEqual(runRequest(both, "ticket", { body: "typed before the entry moved", audience: "support" }, '{"body":"It broke"}'), {
    kind: "test",
    body: { inputs: { audience: "support" }, start: "ticket", event: { body: "It broke" } },
  }, "an input the event fills is never sent from the form");
  // A required input left blank: a sentence under it, nothing sent.
  assert.deepEqual(runRequest(both, BY_HAND, { body: "It broke", audience: "" }, ""), { kind: "refused", errors: { audience: "Required." }, payloadError: null });
  assert.deepEqual(runRequest(both, "ticket", { audience: "" }, '{"body":"x"}'), { kind: "refused", errors: { audience: "Required." }, payloadError: null });
  // A payload that is not JSON: said under the box, with what else is wrong.
  const bad = runRequest(both, "ticket", { audience: "" }, "{");
  assert.equal(bad.kind, "refused");
  assert.deepEqual(bad.errors, { audience: "Required." });
  assert.match(bad.payloadError, /^Not JSON: /);
  // An entry that is no way in sends nothing.
  assert.deepEqual(runRequest(workflow([onHook]), BY_HAND, { audience: "support" }, ""), { kind: "refused", errors: {}, payloadError: null }, "only events begin it: by hand is no entry");
  assert.deepEqual(runRequest(both, "gone", { body: "x", audience: "y" }, "{}"), { kind: "refused", errors: {}, payloadError: null });
});

test("a refusal reads in the node's words — and a workflow that reads its goal is refused by name, and the row is read again", () => {
  const sentence = "the workflow cannot start: step `draft` reads the goal it serves";
  const needsGoal = { error: sentence, problems: [{ step: "draft", kind: "needs_goal", text: { id: "problem-needs-goal", args: { step: "draft" } } }, { step: "tell", kind: "needs_goal", text: { id: "problem-needs-goal", args: { step: "tell" } } }] };
  assert.deepEqual(runRefusal(needsGoal, sentence), {
    words: "It runs on a goal only: draft and tell read the goal they serve, and a run in the workspace has none.",
    needsGoal: true,
    reread: true,
  });
  assert.equal(
    runRefusal({ error: sentence, problems: [needsGoal.problems[0]] }, sentence).words,
    "It runs on a goal only: draft reads the goal it serves, and a run in the workspace has none.",
  );
  // Another problem the row did not know of: the node's sentence, and the row is read again.
  assert.deepEqual(runRefusal({ error: "the workflow has problems", problems: [{ step: "a", kind: "unknown_step", text: { id: "problem-unknown-step" } }] }, "the workflow has problems"), {
    words: "the workflow has problems",
    needsGoal: false,
    reread: true,
  });
  // A refusal that names no problem — an input that does not bind, the node away — is said as it came.
  assert.deepEqual(runRefusal({ error: "input `audience` is required" }, "input `audience` is required"), { words: "input `audience` is required", needsGoal: false, reread: false });
  assert.deepEqual(runRefusal(undefined, "The node is unreachable."), { words: "The node is unreachable.", needsGoal: false, reread: false });
  assert.deepEqual(runRefusal("not a body", "400 Bad Request"), { words: "400 Bad Request", needsGoal: false, reread: false });
});

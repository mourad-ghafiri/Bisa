/**
 * What the goal page decides before it draws. Run with
 * `node --test desktop/src/views/_goal/goalPageModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { ADOPT_SUBJECT, adoptGateOf, closeBody, closeWords, headerWords, isProposed, listeningFacts, ownDesignOf, pageFacts, replacements, runStanding } from "./goalPageModel.mjs";

const ID = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

test("the header names the goal by its words — the title over the statement, else the statement alone — and never by its id", () => {
  assert.deepEqual(headerWords({ id: ID, title: "Ship the report", statement: "Ship the quarterly report to the board." }), { title: "Ship the report", subtitle: "Ship the quarterly report to the board." });
  assert.deepEqual(headerWords({ id: ID, title: null, statement: "Ship the quarterly report to the board." }), { title: "Ship the quarterly report to the board.", subtitle: null }, "an untitled goal is its sentence, said once");
  assert.deepEqual(headerWords({ id: ID, statement: "Ship the quarterly report to the board." }), { title: "Ship the quarterly report to the board.", subtitle: null }, "a row with no title key");
  assert.deepEqual(headerWords({ id: ID, title: "  ", statement: "  Fix the login.  " }), { title: "Fix the login.", subtitle: null }, "blank words are no words; the statement is trimmed");
  assert.deepEqual(headerWords({ id: ID, title: "Fix the login.", statement: "Fix the login." }), { title: "Fix the login.", subtitle: null }, "a statement that only repeats the title is not said twice");
  assert.deepEqual(headerWords({ id: ID, title: "Fix the login", statement: "" }), { title: "Fix the login", subtitle: null });
  assert.deepEqual(headerWords({ id: ID, title: null, statement: "" }), { title: "Untitled goal", subtitle: null }, "the shape's fallback: a sentence, not the id");
  assert.deepEqual(headerWords(null), { title: "Untitled goal", subtitle: null });
  for (const goal of [{ id: ID, title: null, statement: null }, { id: ID, title: "Ship it", statement: "Ship it now." }, { id: ID, statement: "Ship it now." }]) {
    const words = headerWords(goal);
    assert.ok(!words.title.includes(ID) && !words.title.includes(ID.slice(-6)) && !(words.subtitle ?? "").includes(ID.slice(-6)), `the id is nowhere in the words: ${JSON.stringify(words)}`);
  }
});

const adopt = { gate: "approval", subject: "adopt:wf-1", gate_id: "G1" };
const designed = (over = {}) => ({ id: "K", mode: "guided", workflow: "wf-1", run: null, closed: null, ...over });

test("a run is finished by an outcome or a cancel, unfinished while it has neither, and nothing when there is none", () => {
  assert.deepEqual(runStanding(null), { finished: false, unfinished: false });
  assert.deepEqual(runStanding(undefined), { finished: false, unfinished: false });
  assert.deepEqual(runStanding({ outcome: null }), { finished: false, unfinished: true });
  assert.deepEqual(runStanding({}), { finished: false, unfinished: true }, "a run the node sent without the fields is still running");
  for (const outcome of ["done", "failed"]) assert.deepEqual(runStanding({ outcome }), { finished: true, unfinished: false });
  assert.deepEqual(runStanding({ outcome: null, cancelled: { at: 1 } }), { finished: true, unfinished: false }, "cancelled has no outcome and is over");
});

test("the adoption's gate is an approval whose subject names an adoption — never another approval, never a question", () => {
  assert.equal(adoptGateOf([adopt]), adopt);
  assert.equal(adoptGateOf([{ gate: "approval", subject: "step:R/review" }, adopt]), adopt);
  assert.equal(adoptGateOf([{ gate: "escalation", subject: "adopt:wf-1" }]), undefined, "a question wearing the subject is not the gate");
  assert.equal(adoptGateOf([{ gate: "approval", subject: null }, { gate: "approval" }, null]), undefined, "a gate with no subject is read, never thrown on");
  assert.equal(adoptGateOf(null), undefined);
  assert.equal(adoptGateOf("gates"), undefined);
  assert.ok(ADOPT_SUBJECT.endsWith(":"));
});

test("a design waits on the person only when all four hold: a designing mode, a workflow, no run, the gate open", () => {
  assert.equal(isProposed(designed(), [adopt]), true);
  assert.equal(isProposed(designed({ mode: "auto" }), [adopt]), true, "an auto goal whose adoption still reached a person");
  assert.equal(isProposed(designed({ mode: "manual" }), [adopt]), false, "nobody designs a manual goal");
  assert.equal(isProposed(designed({ workflow: null }), [adopt]), false);
  assert.equal(isProposed(designed({ run: "R1" }), [adopt]), false, "a goal that ran is past its proposal");
  assert.equal(isProposed(designed(), []), false, "adopted or declined: the gate is gone");
  assert.equal(isProposed(null, [adopt]), false);
});

test("only the goal's own design is promoted: a library workflow is already in the library", () => {
  const own = { id: "wf-1", origin: { origin: "goal" } };
  assert.equal(ownDesignOf(own), own);
  assert.equal(ownDesignOf({ id: "wf-2", origin: { origin: "library" } }), null);
  assert.equal(ownDesignOf({ id: "wf-3" }), null, "a workflow with no origin is nobody's own");
  assert.equal(ownDesignOf(null), null);
});

test("the goal's listening and its design's ways in: an event design is started by listening, and Run now begins at its start by hand", () => {
  const start = (id, event) => ({ id, name: id, kind: "start", on: { event }, then: [{ to: "work" }] });
  const work = { id: "work", name: "Work", kind: "agent", instructions: "go", then: [] };
  const both = { origin: { origin: "goal" }, steps: [start("by-hand", "manual"), start("weekly", "schedule"), work] };
  assert.deepEqual(listeningFacts(designed(), both), { listening: null, listens: true, manualEntry: "by-hand" });
  const armed = designed({ listening: { since: 5 } });
  assert.deepEqual(listeningFacts(armed, { steps: [start("weekly", "schedule"), work] }), { listening: { since: 5 }, listens: true, manualEntry: null }, "only events begin it");
  assert.deepEqual(listeningFacts(designed(), { steps: [work] }), { listening: null, listens: false, manualEntry: "work" }, "a design with no start begins by hand at its root");
  assert.deepEqual(listeningFacts(null, null), { listening: null, listens: false, manualEntry: null });
  const facts = pageFacts({ goal: armed, run: null, pendingGates: [], startable: both });
  assert.deepEqual([facts.listens, facts.manualEntry, facts.listening], [true, "by-hand", { since: 5 }]);
});

test("the page's facts are one reading, and the view computes none of them again", () => {
  const facts = pageFacts({ goal: designed({ closed: { at: 9 } }), run: null, pendingGates: [adopt], startable: { id: "wf-1", origin: { origin: "goal" } } });
  assert.deepEqual([facts.closed, facts.finished, facts.unfinished, facts.proposed], [true, false, false, true]);
  assert.equal(facts.adoptGate, adopt);
  assert.equal(facts.ownDesign.id, "wf-1");
  const view = readFileSync(new URL("../GoalDetail.tsx", import.meta.url), "utf8");
  assert.ok(view.includes("pageFacts({"));
  assert.ok(!view.includes('startsWith("adopt:")') && !view.includes("run.outcome != null"), "no second formula in the view");
});

const row = (id, over = {}) => ({ id, title: null, statement: `the work of ${id}`, status: "draft", archived: null, ...over });
const others = [row("G1", { title: "Ship the report" }), row("G2", { title: "  " }), row("G3", { status: "closed" }), row("G4", { status: "closed", archived: { at: 3 } }), row("G5", { status: "running", title: "The new report" }), row("G6", { statement: "" })];

test("a goal is replaced by a goal that still stands: never itself, never one closed or put away — each by its title, else its statement, else its id", () => {
  assert.deepEqual(replacements(others, "G1"), [
    { id: "G2", label: "the work of G2" },
    { id: "G5", label: "The new report" },
    { id: "G6", label: "G6" },
  ]);
  assert.deepEqual(replacements(others, "G5").map((g) => g.id), ["G1", "G2", "G6"]);
  for (const nothing of [null, undefined, [], "G1"]) assert.deepEqual(replacements(nothing, "G1"), []);
  assert.deepEqual(replacements([null, row("G9")], "G1").map((g) => g.id), ["G9"], "a row that is none is passed over");
});

test("a close sends the person's word on why, or the goal that replaces this one as `superseded_by` — never both, never an empty string", () => {
  assert.deepEqual(closeBody({ rationale: "  the client left  ", replacedBy: null }, "G1"), { rationale: "the client left" });
  assert.deepEqual(closeBody({ rationale: "", replacedBy: "G5" }, "G1"), { superseded_by: "G5" });
  assert.deepEqual(closeBody({ rationale: "written before the goal was chosen", replacedBy: "G5" }, "G1"), { superseded_by: "G5" }, "a goal replaced is superseded: the node keeps no reason beside the goal that replaces it");
  // Nothing said is nothing sent: the node reads an absent key, and would refuse `""` as no goal's id.
  for (const form of [{ rationale: "", replacedBy: null }, { rationale: "   ", replacedBy: "" }, { rationale: null, replacedBy: "  " }, {}, null, undefined]) assert.deepEqual(closeBody(form, "G1"), {});
  assert.deepEqual(closeBody({ rationale: "done elsewhere", replacedBy: "G1" }, "G1"), { rationale: "done elsewhere" }, "a goal never replaces itself");
  assert.deepEqual(closeBody({ rationale: 7, replacedBy: 9 }, "G1"), {}, "what is no word is no word");
  // A draft that carries more sends only the two keys the node declares.
  assert.deepEqual(Object.keys(closeBody({ rationale: "x", replacedBy: null, open: true, touched: 3 }, "G1")), ["rationale"]);
});

test("the dialog says what will be recorded, and the toast what was: abandoned, or superseded by the goal's name", () => {
  assert.deepEqual(closeWords({ rationale: "the client left", replacedBy: null }, others, "G1"), { records: "The goal is recorded as abandoned — it can be read, never resumed.", closed: "Closed." });
  assert.deepEqual(closeWords({ rationale: "", replacedBy: "G5" }, others, "G1"), { records: "The goal is recorded as superseded by The new report — it can be read, never resumed.", closed: "Closed — superseded by The new report." });
  assert.equal(closeWords({ replacedBy: "G404" }, others, "G1").closed, "Closed — superseded by G404.", "a goal the list has not read yet is said by its id");
  assert.equal(closeWords({ replacedBy: "G1" }, others, "G1").closed, "Closed.", "what is sent and what is said are one rule");
});

test("the page closes through the model: the body is built by name, and the dialog forgets what was said when it is opened again", () => {
  const view = readFileSync(new URL("../GoalDetail.tsx", import.meta.url), "utf8");
  assert.ok(view.includes("api.closeGoal(id, closeBody(closing, id))"), "the body is the model's");
  assert.ok(view.includes("replacements(ws.goals, id)") && view.includes("closeWords(closing, ws.goals, id)"));
  assert.ok(view.includes("setClosing(NOTHING_SAID)"), "a dialog opened again starts from nothing");
  assert.ok(!view.includes("superseded_by"), "the wire's key is written in one place");
});

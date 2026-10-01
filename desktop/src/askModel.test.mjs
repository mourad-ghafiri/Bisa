/**
 * What the answer form decides before it calls the node.
 *
 * Nothing here renders — there is no jsdom in this repo, which is why these
 * decisions are a module rather than a pile of `useState` reads. The ones that
 * can actually be wrong in a way a person notices: what body a selection
 * sends, when Send is live, what "I'm not sure" says on the wire, and whether
 * an option list survives the asker's mistakes.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  answerSummary,
  askOptions,
  canSubmit,
  decideBody,
  isAnswerAsk,
  isMulti,
  keepOffered,
  toggleChoice,
} from "./askModel.mjs";

const OPTIONS = [
  { id: "postgres", label: "Postgres", detail: "one more service to run", recommended: false },
  { id: "sqlite", label: "SQLite", recommended: true },
  { id: "none", label: "No database yet" },
];

/** A question with options, single choice, on a live gate. */
const asking = (over = {}) => ({
  gate_id: "gate-7",
  expects: { kind: "answer", options: OPTIONS, ...over },
});

/** A contract gate: approve or decline, nothing else. */
const gate = { gate_id: "gate-7", expects: { kind: "decision" } };

const empty = { selected: [], text: "" };

// ---------------------------------------------------------------------------
// Which shape is being asked for
// ---------------------------------------------------------------------------

test("a question is an answer ask and a gate is not", () => {
  assert.equal(isAnswerAsk({ kind: "answer" }), true);
  assert.equal(isAnswerAsk({ kind: "decision" }), false);
});

test("anything that is not explicitly an answer is treated as a decision", () => {
  // The conservative half. Showing Approve/Decline for a real question is a
  // mistake a person can see and refuse to make; showing an answer box for a
  // contract gate would resolve a signature with prose.
  assert.equal(isAnswerAsk(undefined), false);
  assert.equal(isAnswerAsk("text"), false);
  assert.equal(isAnswerAsk({ kind: "whatever-comes-next" }), false);
});

// ---------------------------------------------------------------------------
// The option list
// ---------------------------------------------------------------------------

test("options keep the order the asker put them in", () => {
  // Not floated to the top by `recommended`: an asker's ordering carries
  // meaning it never states, and re-sorting around a flag destroys it.
  assert.deepEqual(
    askOptions(asking().expects).map((o) => o.id),
    ["postgres", "sqlite", "none"],
  );
});

test("only the first recommendation survives", () => {
  const opts = askOptions({
    kind: "answer",
    options: [
      { id: "a", label: "A", recommended: true },
      { id: "b", label: "B", recommended: true },
    ],
  });
  assert.deepEqual(
    opts.map((o) => o.recommended),
    [true, false],
  );
});

test("an option with no id is dropped, and so is a repeat of one", () => {
  const opts = askOptions({
    kind: "answer",
    options: [{ id: "a", label: "A" }, { id: "  ", label: "Ghost" }, { label: "Nameless" }, { id: "a", label: "A again" }],
  });
  assert.deepEqual(
    opts.map((o) => o.id),
    ["a"],
  );
});

test("an option with no label reads as its id rather than as a blank row", () => {
  const [o] = askOptions({ kind: "answer", options: [{ id: "postgres" }] });
  assert.equal(o.label, "postgres");
  assert.equal(o.detail, null);
});

test("a decision offers no options however many are attached to it", () => {
  assert.deepEqual(askOptions({ kind: "decision", options: OPTIONS }), []);
});

test("multi means nothing on a question that offered no options", () => {
  assert.equal(isMulti({ kind: "answer", multi: true }), false);
  assert.equal(isMulti(asking({ multi: true }).expects), true);
  assert.equal(isMulti(asking().expects), false);
});

// ---------------------------------------------------------------------------
// Choosing
// ---------------------------------------------------------------------------

test("a single-choice question replaces the choice instead of refusing it", () => {
  assert.deepEqual(toggleChoice(["postgres"], "sqlite", false), ["sqlite"]);
});

test("a multi-choice question accumulates, and unpicking works in both", () => {
  assert.deepEqual(toggleChoice(["postgres"], "sqlite", true), ["postgres", "sqlite"]);
  assert.deepEqual(toggleChoice(["postgres", "sqlite"], "postgres", true), ["sqlite"]);
  assert.deepEqual(toggleChoice(["postgres"], "postgres", false), []);
});

test("an id the question does not offer never reaches the wire", () => {
  // The engine matches by id. An id it never issued resolves to nothing at the
  // far end — not a harmless extra, a choice that silently did not count.
  assert.deepEqual(keepOffered(asking().expects, ["postgres", "mysql"]), ["postgres"]);
  assert.deepEqual(keepOffered(asking({ multi: true }).expects, ["postgres", "none"]), [
    "postgres",
    "none",
  ]);
});

test("a single-choice question caps a stale multi-selection at one", () => {
  assert.deepEqual(keepOffered(asking().expects, ["postgres", "none"]), ["postgres"]);
});

// ---------------------------------------------------------------------------
// When Send is live
// ---------------------------------------------------------------------------

test("a question needs either a choice or a sentence, and neither needs the other", () => {
  assert.equal(canSubmit(asking(), empty), false);
  assert.equal(canSubmit(asking(), { selected: ["sqlite"], text: "" }), true);
  assert.equal(canSubmit(asking(), { selected: [], text: "whatever is cheapest" }), true);
});

test("whitespace is not an answer", () => {
  assert.equal(canSubmit(asking(), { selected: [], text: "   \n " }), false);
});

test("a question with no options at all still sends on text alone", () => {
  const open = { gate_id: "g", expects: { kind: "answer" } };
  assert.equal(canSubmit(open, empty), false);
  assert.equal(canSubmit(open, { selected: [], text: "EUR" }), true);
});

test("a gate approves with no rationale at all", () => {
  // A gate that would not approve without an essay is asking for one to say yes.
  assert.equal(canSubmit(gate, empty), true);
});

// ---------------------------------------------------------------------------
// What gets sent
// ---------------------------------------------------------------------------

test("a chosen option and a sentence travel together, not instead of each other", () => {
  assert.deepEqual(decideBody(asking(), { selected: ["sqlite"], text: "  but revisit in Q3  " }), {
    approve: true,
    gate: "gate-7",
    answer: { selected: ["sqlite"], text: "but revisit in Q3", unsure: false },
  });
});

test("a step-scoped ask names its step on the wire, a goal-scoped one does not, and a durable ask carries no gate", () => {
  const release = { gate_id: null, step: "hold", durable: true, expects: { kind: "decision" } };
  assert.deepEqual(decideBody(release, empty), { approve: true, step: "hold" }, "a held step is released by name");
  const amend = { gate_id: null, step: null, durable: true, subject: "amend:r@w", expects: { kind: "decision" } };
  assert.deepEqual(decideBody(amend, empty), { approve: true }, "an amendment is the goal's, not a step's");
  assert.deepEqual(decideBody({ ...asking(), step: "ask" }, { selected: ["sqlite"], text: "" }), {
    approve: true,
    gate: "gate-7",
    step: "ask",
    answer: { selected: ["sqlite"], unsure: false },
  });
  assert.equal("step" in decideBody({ ...asking(), step: "" }, { selected: ["sqlite"], text: "" }), false, "an empty step is no step");
});

test("an answer with nothing in it is not a body", () => {
  assert.equal(decideBody(asking(), empty), null);
});

test("an empty text field is left off the wire rather than sent as a blank", () => {
  const body = decideBody(asking(), { selected: ["none"], text: "  " });
  assert.deepEqual(body.answer, { selected: ["none"], unsure: false });
  assert.equal("text" in body.answer, false);
});

test("I'm not sure is an approval, never a decline", () => {
  // `approve: false` reaches the waiting agent as "The human DENIED. Do not
  // proceed" — the one thing a person who simply does not know must not say.
  const body = decideBody(asking(), { selected: [], text: "", unsure: true });
  assert.equal(body.approve, true);
  assert.deepEqual(body.answer, { selected: [], unsure: true });
});

test("I'm not sure sends with no choice and no text at all", () => {
  assert.notEqual(decideBody(asking(), { selected: [], text: "", unsure: true }), null);
});

test("I'm not sure drops ticks the person was abandoning, and keeps what they typed", () => {
  const body = decideBody(asking(), {
    selected: ["sqlite"],
    text: "depends what the ops team says",
    unsure: true,
  });
  assert.deepEqual(body.answer, {
    selected: [],
    text: "depends what the ops team says",
    unsure: true,
  });
});

test("a question never sends approve false, whatever the form is holding", () => {
  const body = decideBody(asking(), { selected: ["sqlite"], text: "", approve: false });
  assert.equal(body.approve, true);
});

test("a gate sends a rationale, and a question never does", () => {
  assert.deepEqual(decideBody(gate, { selected: [], text: " scope looks right ", approve: true }), {
    approve: true,
    gate: "gate-7",
    rationale: "scope looks right",
  });
  const answered = decideBody(asking(), { selected: [], text: "EUR" });
  assert.equal("rationale" in answered, false);
});

test("a declined gate carries its reason", () => {
  assert.deepEqual(decideBody(gate, { selected: [], text: "too broad", approve: false }), {
    approve: false,
    gate: "gate-7",
    rationale: "too broad",
  });
});

test("a durable ask is decided by goal, so it sends no gate id", () => {
  const durable = { gate_id: null, expects: { kind: "answer" } };
  const body = decideBody(durable, { selected: [], text: "EUR" });
  assert.equal("gate" in body, false);
});

// ---------------------------------------------------------------------------
// Reading an answer back
// ---------------------------------------------------------------------------

test("a recorded answer reads as one phrase in an activity line", () => {
  assert.equal(answerSummary({ selected: ["sqlite"], text: "for now", unsure: false }), "chose sqlite — “for now”");
  assert.equal(answerSummary({ selected: ["a", "b"], unsure: false }), "chose a, b");
  assert.equal(answerSummary({ selected: [], text: "EUR", unsure: false }), "“EUR”");
  assert.equal(answerSummary({ selected: [], unsure: true }), "not sure");
  assert.equal(
    answerSummary({ selected: [], text: "ask the ops team", unsure: true }),
    "not sure — “ask the ops team”",
  );
});

test("an answer that said nothing summarises to nothing, not to empty quotes", () => {
  assert.equal(answerSummary({ selected: [], unsure: false }), null);
  assert.equal(answerSummary(null), null);
  // The old wire form was a bare string. A row rendering one must not print
  // `[object Object]`, and it must not print `“undefined”` either.
  assert.equal(answerSummary("yes, ship it"), null);
});

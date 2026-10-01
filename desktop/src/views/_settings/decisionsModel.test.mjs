/**
 * Settings › Decision Making's words and shapes, tested where they live. Nothing
 * here renders; what can be wrong in a way a person notices is a provider
 * showing the wrong fields, a point that can be switched off when it should
 * not, or a *Try it* request built with fewer options than a choice needs.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  KEYS,
  POINTS,
  POINT_IDS,
  answerLines,
  blankKey,
  blankTryForm,
  calibratedNote,
  coreLine,
  fieldsFor,
  judgementLine,
  keyCleared,
  keyFor,
  keySaved,
  keyTyped,
  mayClearKey,
  maySaveKey,
  movesJudgements,
  movesStatus,
  offList,
  pointIsOn,
  pointLabel,
  pointRows,
  pointsOffAfter,
  readyLine,
  takesKey,
  tryProblem,
  tryRequest,
} from "./decisionsModel.mjs";

test("a provider shows only its own fields", () => {
  assert.deepEqual(fieldsFor("harness"), [KEYS.harness.id, KEYS.harness.model, KEYS.harness.effort]);
  assert.equal(KEYS.harness.effort, "decisions.harness.effort", "how hard the judge's own model works, beside the model");
  assert.deepEqual(fieldsFor("agent"), [KEYS.agent.id]);
  assert.deepEqual(fieldsFor("jev"), [KEYS.jev.model]);
  assert.deepEqual(fieldsFor("rlcd"), [KEYS.rlcd.endpoint, KEYS.rlcd.model, KEYS.rlcd.auth]);
  assert.deepEqual(fieldsFor("nonesuch"), []);
  // rlcdAuth does not change which fields show — only whether a key is asked for.
  assert.deepEqual(fieldsFor("rlcd", "none"), fieldsFor("rlcd", "bearer"));
});

test("only Jev always takes a key; RLCD only over bearer; a harness or an agent never do", () => {
  assert.equal(takesKey("jev"), true);
  assert.equal(takesKey("jev", "none"), true);
  assert.equal(takesKey("rlcd", "bearer"), true);
  assert.equal(takesKey("rlcd", "none"), false);
  assert.equal(takesKey("rlcd"), false);
  assert.equal(takesKey("harness"), false);
  assert.equal(takesKey("agent"), false);
});

/** `DecisionPoint::ALL`, as the wire words `as_str` gives its variants, in the core's order. */
function pointsInRust() {
  const src = readFileSync(new URL("../../../../crates/bisa-core/src/decision.rs", import.meta.url), "utf8");
  const words = new Map([...src.matchAll(/DecisionPoint::([A-Z][A-Za-z]+) => "([a-z._]+)",/g)].map((m) => [m[1], m[2]]));
  const at = src.indexOf("pub const ALL: [DecisionPoint;");
  assert.ok(at >= 0, "DecisionPoint::ALL is declared");
  return [...src.slice(at, src.indexOf("];", at)).matchAll(/DecisionPoint::([A-Z][A-Za-z]+),/g)].map((m) => words.get(m[1]));
}

test("every one of the eleven points has a label and a sentence, none a fragment, in the order the node lists them", () => {
  assert.deepEqual([...POINT_IDS], ["model.route", "model.effort", "security.tool", "security.message", "security.content", "assign.pick", "dispatch.triage", "goal.adopt", "browser.headless", "workflow.judge", "agent.decide"]);
  assert.equal(POINT_IDS.length, 11);
  assert.deepEqual([...POINT_IDS], pointsInRust(), "the core's points, in the core's order");
  for (const id of POINT_IDS) {
    assert.ok(POINTS[id].label.length > 3, `${id} has a label`);
    assert.ok(POINTS[id].description.length > 20, `${id} has a sentence`);
    assert.equal(pointLabel(id), POINTS[id].label);
  }
  assert.equal(pointLabel("nonesuch"), "nonesuch");
  assert.equal(pointLabel(null), "");
});

test("switching a point edits the points-off list, idempotently", () => {
  assert.deepEqual(pointsOffAfter([], "assign.pick", false), ["assign.pick"]);
  assert.deepEqual(pointsOffAfter(["assign.pick"], "assign.pick", true), []);
  assert.deepEqual(pointsOffAfter(["assign.pick"], "assign.pick", false), ["assign.pick"], "switching off twice is once");
  assert.deepEqual(pointsOffAfter(["dispatch.triage"], "assign.pick", false).sort(), ["assign.pick", "dispatch.triage"].sort());
});

test("a point is on when it is not switched off, or when it was never a person's to switch", () => {
  assert.equal(pointIsOn({ point: "assign.pick", selected_explicitly: false, on: true }), true);
  assert.equal(pointIsOn({ point: "assign.pick", selected_explicitly: false, on: false }), false);
  assert.equal(pointIsOn({ point: "model.route", selected_explicitly: true, on: false }), true, "an explicitly selected point is always on");
  assert.equal(pointIsOn(null), false);
});

test("how hard a model works is the second point, selected where it is used as the route is — never a switch", () => {
  assert.equal(POINT_IDS[0], "model.route");
  assert.equal(POINT_IDS[1], "model.effort");
  assert.equal(POINTS["model.effort"].label, "How hard a model works");
  assert.equal(POINTS["model.effort"].description, "Under an effort of Auto, which level a model works at for a task — a level somebody named still wins.");
  assert.equal(pointLabel("model.effort"), "How hard a model works");
  // The node says it is selected explicitly: on whatever the switches say, and never in the off list.
  assert.equal(pointIsOn({ point: "model.effort", selected_explicitly: true, on: false }), true);
  const core = readFileSync(new URL("../../../../crates/bisa-core/src/decision.rs", import.meta.url), "utf8");
  const at = core.indexOf("pub fn is_selected_explicitly(self) -> bool {");
  assert.ok(at >= 0, "the core says which points are selected by name");
  const arms = core.slice(at, core.indexOf("\n    }\n", at));
  assert.ok(arms.includes("DecisionPoint::ModelRoute") && arms.includes("DecisionPoint::ModelEffort"), "both model points are");
  const panel = readFileSync(new URL("./DecisionsPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("pointRows(status.data, busy)") && panel.includes("disabled={row.held}"), "the panel draws the model's rows, and holds the switch the model holds");
  assert.equal(pointRows({ enabled: true, points: [{ point: "model.effort", selected_explicitly: true, on: true }] }, null).find((r) => r.id === "model.effort").held, true, "a point selected by name is never the panel's to switch");
});

test("the points that speak of who judges name the Decision-Making Agent", () => {
  assert.equal(POINTS["security.tool"].description, "When the classifier's provider is the Decision-Making Agent, whether a tool call is safe to run or harmful and why.");
  assert.match(POINTS["agent.decide"].description, /may ask the Decision-Making Agent a question of its own/);
});

test("the readiness line is off, ready, or why not — and never guesses before the status arrives", () => {
  assert.equal(readyLine(null).tone, "quiet");
  assert.equal(readyLine(null).text, "Reading the Decision-Making Agent…");
  const ready = readyLine({ enabled: true, ready: true, answers_as: "the harness claude-code · claude-sonnet-5-5[1m]" });
  assert.equal(ready.tone, "ok");
  assert.match(ready.text, /claude-sonnet-5-5\[1m\]/);
  const notReady = readyLine({ enabled: true, ready: false, problem: "no key is stored for jev" });
  assert.equal(notReady.tone, "warn");
  assert.equal(notReady.text, "no key is stored for jev");
  assert.deepEqual(readyLine({ enabled: true, ready: false }), { tone: "warn", text: "Not ready." });
});

test("off for the workspace is not off everywhere: a point selected by name still asks, so what cannot be asked is still said", () => {
  // 15 §Where it is on: the switch is one of four ways. A `judge` step, an `auto_route` plan, an effort of `auto`, the
  // classifier's provider, an agent's or a workflow's own switch — each asks whatever `decisions.enabled` says.
  const off = readyLine({ enabled: false, ready: true, answers_as: "jev-latest" });
  assert.deepEqual(off, { tone: "quiet", text: "Off for the workspace — asked only where it is selected by name, or where an agent or a workflow switches it on. Answers as jev-latest." });
  const offAndNotReady = readyLine({ enabled: false, ready: false, problem: "no key is stored for jev" });
  assert.equal(offAndNotReady.tone, "warn", "a judge step would take `otherwise` every time: the person is told why");
  assert.equal(offAndNotReady.text, "Off for the workspace — and where it is selected it cannot be asked: no key is stored for jev");
  assert.equal(readyLine({ enabled: false, ready: false }).text, "Off for the workspace — and where it is selected it cannot be asked: Not ready.");
});

test("the points as the panel draws them: each with its words, whether it is on, and whether its switch is the person's", () => {
  const status = {
    enabled: true,
    points: [
      { point: "model.route", selected_explicitly: true, on: true },
      { point: "assign.pick", selected_explicitly: false, on: true },
      { point: "dispatch.triage", selected_explicitly: false, on: false },
    ],
  };
  const rows = pointRows(status, null);
  assert.deepEqual(rows.map((r) => r.id), [...POINT_IDS], "every point, in the node's order, whatever the status lists");
  const by = Object.fromEntries(rows.map((r) => [r.id, r]));
  assert.deepEqual([by["model.route"].on, by["model.route"].held, by["model.route"].selected], [true, true, true], "selected where it is used: on, and never a switch of the panel's");
  assert.match(by["model.route"].hint, /On where it is selected\.$/);
  assert.deepEqual([by["assign.pick"].on, by["assign.pick"].held], [true, false]);
  assert.deepEqual([by["dispatch.triage"].on, by["dispatch.triage"].held], [false, false], "switched off: left to its own rule");
  assert.deepEqual([by["goal.adopt"].on, by["goal.adopt"].held], [false, false], "a point the status does not list is off");
  // While a write is on its way every switch waits: a second one would be built on a list the first has yet to change.
  assert.ok(pointRows(status, "assign.pick").every((r) => r.held));
  // The switch off for the workspace: no point is the person's to flip here — it is switched on above first.
  const globalOff = pointRows({ ...status, enabled: false }, null);
  assert.ok(globalOff.every((r) => r.held));
  assert.equal(globalOff.find((r) => r.id === "model.route").on, true, "and a point selected by name is still on");
  assert.deepEqual(pointRows(null, null).map((r) => [r.on, r.held]), POINT_IDS.map(() => [false, true]), "nothing read yet");
});

test("the list a switch is built on is the last one this window wrote, until the node's own read catches up", () => {
  assert.deepEqual(offList(["assign.pick"], null), ["assign.pick"], "nothing written here: what was read");
  assert.deepEqual(offList([], ["assign.pick"]), ["assign.pick"], "written and not yet read back: a second switch builds on it");
  assert.deepEqual(pointsOffAfter(offList([], ["assign.pick"]), "dispatch.triage", false).sort(), ["assign.pick", "dispatch.triage"], "so two switches flipped in a row are both kept");
  assert.deepEqual(offList(undefined, null), []);
  assert.deepEqual(offList("nonsense", null), [], "a value that is no list is none");
});

test("a key typed for one provider is never another's: the box is the provider's own", () => {
  let box = blankKey("jev");
  assert.deepEqual(box, { provider: "jev", typed: "", sent: null });
  box = keyTyped(box, "jev", "not-a-real-key");
  assert.equal(maySaveKey(box, "jev"), true);
  box = keySaved(box, "jev");
  assert.deepEqual(box, { provider: "jev", typed: "not-a-real-key", sent: "not-a-real-key" }, "kept in its box while the window lives");
  assert.equal(maySaveKey(box, "jev"), false, "Save waits for a change");
  // The provider moves to RLCD: the box under it is empty — Jev's key is not drawn, not revealed, not sent.
  assert.deepEqual(keyFor(box, "rlcd"), { provider: "rlcd", typed: "", sent: null });
  assert.equal(maySaveKey(box, "rlcd"), false);
  const moved = keyTyped(box, "rlcd", "x");
  assert.deepEqual(moved, { provider: "rlcd", typed: "x", sent: null }, "typing under the new provider starts from nothing");
  // And back: what was typed for Jev is gone with the move — a key is typed again, never carried.
  assert.deepEqual(keyFor(moved, "jev"), { provider: "jev", typed: "", sent: null });
  assert.equal(maySaveKey(keyTyped(blankKey("jev"), "jev", "   "), "jev"), false, "blank is no key");
  assert.deepEqual(keyCleared("jev"), blankKey("jev"));
  assert.equal(mayClearKey({ key_stored: true }), true);
  assert.equal(mayClearKey({ key_stored: false }), false);
  assert.equal(mayClearKey({ key_stored: null }), false);
  assert.equal(mayClearKey(null), false);
});

test("what moves the panel: a decisions.* setting written, a judgement recorded — and nothing else", () => {
  assert.ok(movesStatus({ type: "settings_changed", scope: "workspace", keys: ["decisions.provider"] }));
  assert.ok(movesStatus({ type: "settings_changed", scope: "machine", keys: ["appearance.theme", "decisions.enabled"] }));
  assert.ok(!movesStatus({ type: "settings_changed", scope: "workspace", keys: ["security.classifier.provider"] }));
  assert.ok(!movesStatus({ type: "settings_changed" }), "a frame that names no key");
  assert.ok(movesStatus({ type: "judged" }), "a judgement says whether the provider answered");
  assert.ok(!movesStatus({ type: "step_changed" }));
  assert.ok(!movesStatus(null));
  assert.ok(movesJudgements({ type: "judged" }));
  assert.ok(!movesJudgements({ type: "settings_changed", keys: ["decisions.enabled"] }));
  assert.ok(!movesJudgements(null));
});

test("the calibration note is silent for a calibrated model and plain otherwise", () => {
  assert.equal(calibratedNote(null), null);
  assert.equal(calibratedNote({ calibrated: true }), null);
  assert.match(calibratedNote({ calibrated: false }), /own estimate/);
});

test("a choice needs two options; a noul needs only its instructions", () => {
  const blank = blankTryForm();
  assert.equal(blank.kind, "noul");
  assert.match(tryProblem({ ...blank, instructions: "" }), /instructions/);
  assert.equal(tryProblem({ ...blank, instructions: "is this urgent?" }), null);
  assert.match(tryProblem({ ...blank, kind: "choice", instructions: "which team?", options: [{ id: "a", meaning: "x" }] }), /two options/);
  assert.equal(
    tryProblem({ ...blank, kind: "choice", instructions: "which team?", options: [{ id: "a", meaning: "x" }, { id: "b", meaning: "y" }] }),
    null,
  );
});

test("a noul request has one question; a choice request's criteria drop blank rows", () => {
  const noul = tryRequest({ state: "the customer asked for a refund", kind: "noul", instructions: "is this urgent?", options: [] });
  assert.deepEqual(noul, { state: "the customer asked for a refund", questions: { q: { type: "noul", instructions: "is this urgent?" } } });
  const choice = tryRequest({
    state: "{}",
    kind: "choice",
    instructions: "which team?",
    options: [{ id: "billing", meaning: "a payment problem" }, { id: "", meaning: "" }, { id: "support", meaning: "everything else" }],
  });
  assert.deepEqual(choice.questions.q, {
    type: "choice",
    instructions: "which team?",
    criteria: { billing: "a payment problem", support: "everything else" },
  });
});

test("answers read as one line each, by type — a noul is a probability, not a plain yes/no", () => {
  assert.deepEqual(
    answerLines({
      answers: {
        a: { type: "noul", noul: 0.91 },
        b: { type: "choice", choice: "billing", confidence: 0.91 },
        c: { type: "score", score: 2, legend: { "2": "medium" }, confidence: 0.6 },
        d: { type: "noul", noul: 0.2 },
      },
    }),
    ["a: yes (82% sure)", "b: billing (91% sure)", "c: 2 — medium (60% sure)", "d: no (60% sure)"],
  );
  // 0.5 is the least sure a noul answer can be — |0.5 - 0.5| * 2 = 0.
  assert.match(answerLines({ answers: { e: { type: "noul", noul: 0.5 } } })[0], /\(0% sure\)/);
  assert.deepEqual(answerLines(null), []);
});

test("a judgement line names when, the point, the outcome, the answers and the reason", () => {
  const record = {
    seq: 1,
    at: 1000,
    run: "r1",
    step: "assign",
    judgement: {
      point: "assign.pick",
      provider: "jev",
      model: "jev-latest",
      calibrated: true,
      questions: {},
      answers: { pick: { type: "choice", choice: "developer", probabilities: { developer: 0.91 }, confidence: 0.91 } },
      outcome: "applied",
      latency_ms: 120,
      usage: { input_tokens: 40, output_tokens: 8 },
    },
  };
  const line = judgementLine(record, 1030);
  assert.match(line, /30 s ago/);
  assert.match(line, /Who picks up a work item/);
  assert.match(line, /applied/);
  assert.match(line, /developer \(91% sure\)/);
  assert.match(line, /jev-latest/);
  const unsure = judgementLine({ ...record, judgement: { ...record.judgement, outcome: "unsure", reason: "sure to 0.42, and this point acts from 0.70", answers: {} } }, 1030);
  assert.match(unsure, /unsure/);
  assert.match(unsure, /sure to 0\.42/);
});

test("the Decision-Making Agent's card reads off, or on with who answers for it — and says when it could not be read", () => {
  assert.equal(coreLine(null), "");
  assert.equal(coreLine({ enabled: false }), "off");
  assert.equal(coreLine({ enabled: true, answers_as: "the agent general-agent" }), "on for the workspace · the agent general-agent");
  assert.equal(coreLine(null, "the node is unreachable"), "could not be read: the node is unreachable");
  assert.equal(coreLine({ enabled: true, answers_as: "jev-latest" }, "timed out"), "on for the workspace · jev-latest", "the last answer stands while a re-read fails");
  const agents = readFileSync(new URL("../Agents.tsx", import.meta.url), "utf8");
  assert.ok(agents.includes("coreLine(status.data, status.error)"), "the card hands the read's failure to the model");
  const model = readFileSync(new URL("./decisionsModel.mjs", import.meta.url), "utf8");
  assert.ok(!model.includes('return "off"'), "the card's word is the catalog's");
});

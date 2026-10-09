/**
 * Who hears a workflow's start events, as the switch, the Turn on dialog
 * and a goal's header say it: nothing to hear, Off, can't turn on, On with
 * what it listens for and when it next comes due, paused and why; the body
 * turning On sends; a goal's line and its verb. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/listeningModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { dateTime } from "../../i18n/format.mjs";
import { initialValues, inputHint, toRequest, validateInputs } from "./workflowForm.mjs";
import {
  NEXT_DUE_FORMAT,
  againBody,
  eventStartsOf,
  goalListening,
  hasEventStarts,
  listeningFor,
  movesListeningOf,
  neededInputs,
  nextDue,
  nextWords,
  pausedWords,
  readBudget,
  switchState,
  turnOnBlockers,
  turnOnBody,
} from "./listeningModel.mjs";

const manual = { step: "by-hand", event: "manual", summary: { id: "step-summary-start-manual" } };
const hourly = { step: "hourly", event: "schedule", summary: { id: "step-summary-start-every", args: { secs: "3600" } } };
const ticket = { step: "ticket", event: "hook", summary: { id: "step-summary-start-hook" } };

const row = (over = {}, workflow = {}) => ({
  workflow: { id: "01WF", name: "Triage", archived: null, origin: { origin: "workspace" }, steps: [], inputs: [], ...workflow },
  problems: [],
  workspace_problems: [],
  runs: { live: 0, total: 0 },
  used_by: [],
  listening: null,
  starts: [manual, hourly, ticket],
  event_only: false,
  listening_needs: [],
  ...over,
});

const listener = (step, next_due = null) => ({ listener: `workspace:01WF/${step}`, host: "workspace:01WF", step, event: "schedule", summary: hourly.summary, next_due, backlog: 0, live_runs: 0 });

test("a workflow with nothing but a start by hand has nothing to hear: no switch", () => {
  const byHand = row({ starts: [manual] });
  assert.equal(hasEventStarts(byHand), false);
  assert.equal(switchState(byHand).state, "none");
  assert.equal(switchState(null).state, "none");
  assert.deepEqual(eventStartsOf(row()).map((s) => s.step), ["hourly", "ticket"]);
  assert.equal(listeningFor(row()), "begins every 3600 seconds · begins when called", "the node's own words for each event start");
});

test("Off, with nothing in the way: pressing it asks what turning On needs", () => {
  assert.deepEqual(switchState(row()), { state: "off", words: "Off", tone: "quiet", toggle: "on", again: false });
});

test("it can't turn on while it has problems, is put away, or is a goal's own design — and says which", () => {
  const broken = switchState(row({ problems: [{ kind: "bad_timer" }, { kind: "no_start" }] }));
  assert.equal(broken.state, "blocked");
  assert.equal(broken.words, "Can't turn on: 2 problems");
  assert.equal(broken.toggle, null);
  assert.equal(switchState(row({ problems: [{ kind: "bad_timer" }] })).words, "Can't turn on: 1 problem");
  assert.equal(switchState(row({}, { archived: { at: 1 } })).words, "Can't turn on: it is archived");
  assert.deepEqual(turnOnBlockers(row({}, { origin: { origin: "goal", goal: "01G" } })), ["it is a goal's own design — its goal listens"]);
  assert.deepEqual(turnOnBlockers(row()), []);
});

test("On says what it listens for, and when the soonest clock comes due", () => {
  const on = row({ listening: { since: 10 } });
  assert.deepEqual(switchState(on), { state: "on", words: "On — begins every 3600 seconds · begins when called", tone: "ok", toggle: "off", again: false });
  const listeners = [listener("hourly", 5000), listener("ticket"), listener("other", 7000)];
  assert.equal(nextDue(listeners), 5000);
  assert.equal(nextDue([]), null);
  assert.equal(nextWords(5000), dateTime(5000, NEXT_DUE_FORMAT), "a weekday and a time, in this machine's zone");
  assert.equal(switchState(on, listeners).words, `On — begins every 3600 seconds · begins when called · next ${nextWords(5000)}`);
});

test("Paused says why, turns off, and may be heard again with what it listened with", () => {
  const paused = row({ listening: { since: 10, inputs: { team: "ops" }, budget: { max_usd_cents: 500 }, paused: { reason: { reason: "run_failed", run: "01R" }, at: 20 } } });
  const s = switchState(paused);
  assert.equal(s.state, "paused");
  assert.equal(s.words, "Paused: a run it started failed");
  assert.equal(s.tone, "warn");
  assert.equal(s.toggle, "off");
  assert.equal(s.again, true);
  assert.equal(pausedWords({ reason: { reason: "budget_spent" }, at: 1 }), "its budget is spent");
  assert.deepEqual(againBody(paused.listening), { inputs: { team: "ops" }, budget: { max_usd_cents: 500 } });
});

test("turning On asks what the events do not supply, and a per-run budget only when one is set", () => {
  const r = row({ listening_needs: ["command", "interval"] }, { inputs: [{ name: "interval", label: "Interval", kind: "number", required: true }, { name: "ticket", label: "Ticket", kind: "text", required: true }, { name: "command", label: "Command", kind: "text", required: true }] });
  assert.deepEqual(neededInputs(r).map((i) => i.name), ["interval", "command"], "in the workflow's own order");
  assert.deepEqual(turnOnBody({}, null), {});
  assert.deepEqual(turnOnBody({ command: "true" }, { max_usd_cents: "250", max_tokens: "", max_wall_clock_secs: 0 }), { inputs: { command: "true" }, budget: { max_usd_cents: 250 } });
  assert.deepEqual(againBody(null), {});
});

test("a goal's header line: listening for what, next when — or paused, and why", () => {
  assert.equal(goalListening(null), null);
  const listening = goalListening({ since: 1 }, [listener("hourly", 9000)]);
  assert.deepEqual(listening, { paused: false, words: `Listening — begins every 3600 seconds · next ${nextWords(9000)}`, tone: "accent" });
  assert.equal(goalListening({ since: 1 }).words, "Listening", "before its listeners are read");
  assert.equal(goalListening({ since: 1 }, [{ ...listener("hourly"), next_due: null }]).words, "Listening — begins every 3600 seconds");
  const paused = goalListening({ since: 1, paused: { reason: { reason: "run_failed", run: "01R" }, at: 2 } });
  assert.deepEqual(paused, { paused: true, words: "Paused: a run it started failed", tone: "warn" });
});

test("the switch and the header re-read when their host's listening moves, and on nobody else's", () => {
  assert.ok(movesListeningOf({ payload: { type: "listening_changed", host: "workspace:01WF", on: true } }, "workspace:01WF"));
  assert.ok(!movesListeningOf({ payload: { type: "listening_changed", host: "workspace:01OTHER", on: true } }, "workspace:01WF"));
  assert.ok(movesListeningOf({ payload: { type: "listener_failed", listener: "goal:01G/ticket", error: "x" } }, "goal:01G"));
  assert.ok(!movesListeningOf({ payload: { type: "listener_failed", listener: "goal:01GG/ticket", error: "x" } }, "goal:01G"), "a host whose id merely starts the same is another host");
  assert.ok(!movesListeningOf({ payload: { type: "run_started" } }, "workspace:01WF"));
  assert.ok(!movesListeningOf(null, "workspace:01WF"));
});

test("a workflow that reads its goal can't be turned On in the workspace: the switch says so, and no verb offers it", () => {
  // 03-workflows §Listening: turning On is refused for a definition that reads its goal — the row carries why (`workspace_problems`).
  const readsGoal = row({ workspace_problems: [{ step: "tell", kind: "needs_goal", text: { id: "problem-needs-goal" } }] });
  assert.deepEqual(turnOnBlockers(readsGoal), ["a step reads the goal it serves, and a run in the workspace has none"]);
  assert.deepEqual(switchState(readsGoal), { state: "blocked", words: "Can't turn on: a step reads the goal it serves, and a run in the workspace has none", tone: "danger", toggle: null, again: false });
  // With a problem of its own beside it, both are said.
  assert.equal(
    switchState(row({ problems: [{ kind: "bad_timer" }], workspace_problems: [{ step: "tell", kind: "needs_goal" }] })).words,
    "Can't turn on: 1 problem · a step reads the goal it serves, and a run in the workspace has none",
  );
  // One already On — it was turned on before the step was drawn — still says On, and turns Off.
  assert.equal(switchState({ ...readsGoal, listening: { since: 1 } }).state, "on");
});

test("a per-run ceiling as a person types it: dollars, tokens, minutes — blank for none, and never a silent none for what is no number", () => {
  assert.deepEqual(readBudget({ dollars: "", tokens: "", minutes: "" }), { budget: { max_usd_cents: null, max_tokens: null, max_wall_clock_secs: null }, errors: {} });
  assert.deepEqual(readBudget({ dollars: "2.50", tokens: "200000", minutes: "30" }), { budget: { max_usd_cents: 250, max_tokens: 200000, max_wall_clock_secs: 1800 }, errors: {} });
  assert.deepEqual(readBudget({ dollars: " 1,5 ", tokens: "", minutes: "0.5" }).budget, { max_usd_cents: 150, max_tokens: null, max_wall_clock_secs: 30 }, "a comma is a decimal mark");
  // What is typed and is no ceiling is said under its field — the run would otherwise go with none, unseen.
  const wrong = readBudget({ dollars: "five", tokens: "-10", minutes: "0" });
  assert.deepEqual(wrong.errors, {
    dollars: "A ceiling is a number above zero — leave it blank for none.",
    tokens: "A ceiling is a number above zero — leave it blank for none.",
    minutes: "A ceiling is a number above zero — leave it blank for none.",
  });
  assert.deepEqual(wrong.budget, { max_usd_cents: null, max_tokens: null, max_wall_clock_secs: null });
  assert.deepEqual(readBudget({ dollars: "1", tokens: "1.5", minutes: "" }).errors, { tokens: "Tokens are counted whole." });
  assert.deepEqual(readBudget({ dollars: "0.001", tokens: "", minutes: "" }).errors, { dollars: "A ceiling is a number above zero — leave it blank for none." }, "less than a cent is no ceiling");
  assert.deepEqual(readBudget({ dollars: "1e3", tokens: "", minutes: "" }).budget.max_usd_cents, 100000);
  assert.deepEqual(readBudget({ dollars: "Infinity", tokens: "", minutes: "" }).errors, { dollars: "A ceiling is a number above zero — leave it blank for none." });
  assert.deepEqual(readBudget(null), { budget: { max_usd_cents: null, max_tokens: null, max_wall_clock_secs: null }, errors: {} });
  // What it read is what turning On sends.
  assert.deepEqual(turnOnBody({ audience: "the team" }, readBudget({ dollars: "5", tokens: "", minutes: "" }).budget), { inputs: { audience: "the team" }, budget: { max_usd_cents: 500 } });
});

test("the Turn on dialog asks what the node names and no default fills — each as required, so nothing is sent that the node would refuse", () => {
  const inputs = [
    { name: "audience", label: "Audience", kind: "text", required: true },
    { name: "when", label: "When", kind: "text", required: false },
    { name: "channel", label: "Channel", kind: "text", required: true, default: "general" },
  ];
  // `weekly-review` turned on with nothing listens on its Monday morning: what a default fills is never asked.
  assert.deepEqual(neededInputs(row({ listening_needs: [] }, { inputs })), []);
  // An older row, or one read a moment before a default was written, may still name it: the dialog does not ask it.
  const r = row({ listening_needs: ["when", "channel"] }, { inputs });
  const needs = neededInputs(r);
  assert.deepEqual(needs.map((d) => [d.name, d.required]), [["when", true]]);
  // Optional for a run by hand, and the schedule cannot be armed without it: left blank it is said here, not by a refusal.
  const values = initialValues(needs);
  assert.deepEqual(validateInputs(needs, values), { when: "Required." });
  assert.equal(inputHint(needs[0], undefined, "workspace"), "Required.");
  assert.deepEqual(turnOnBody(toRequest(needs, { when: " 0 9 * * 1 " }), readBudget(null).budget), { inputs: { when: "0 9 * * 1" } });
  assert.equal(inputs[1].required, false, "the row is left as it was read");
  assert.deepEqual(neededInputs(null), []);
  const dialog = readFileSync(new URL("./TurnOnDialog.tsx", import.meta.url), "utf8");
  assert.ok(dialog.includes("const needs = neededInputs(row);") && dialog.includes("turnOnBody(toRequest(needs, values), ceiling.budget)"), "the dialog asks, checks and sends the same list");
});

// added by the coverage pass: listeningModel.test.mjs
test("a pause for a reason the model does not know is said in general words", () => {
  assert.equal(pausedWords({ reason: { reason: "weird" }, at: 1 }), "it stopped hearing its events");
  assert.equal(pausedWords(undefined), "it stopped hearing its events");
});

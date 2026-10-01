/**
 * A library card's words: the one state it leads with, its facts line, and
 * the shape of its `⋮`. Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { cardMenu, cardMeta, cardStatus, onMark } from "./workflowCardModel.mjs";
import { workflowVerbs } from "./workflowVerbs.mjs";

const schedule = { step: "weekly", event: "schedule", summary: { id: "step-summary-start-cron", args: { cron: "0 9 * * 1" } } };
const byHand = { step: "by-hand", event: "manual", summary: { id: "step-summary-start-manual" } };

const goal = (id, live) => ({ kind: "goal", id, label: id, live });
const row = (over = {}, workflow = {}) => ({
  workflow: { id: "wf", archived: null, steps: [{ id: "a" }, { id: "b" }, { id: "c" }], inputs: [], origin: { origin: "workspace" }, ...workflow },
  problems: [],
  workspace_problems: [],
  runs: { live: 0, total: 0 },
  used_by: [],
  ...over,
});

test("the state line: archived beats running beats problems beats runs on a goal beats ready", () => {
  assert.deepEqual(cardStatus(row()), { tone: "ok", icon: "ok", words: "ready to run", live: false });
  assert.deepEqual(cardStatus(row({ problems: [{ kind: "no_start" }, { kind: "empty_name" }] })), { tone: "danger", icon: "warn", words: "2 problems", live: false });
  assert.deepEqual(cardStatus(row({ workspace_problems: [{ kind: "needs_goal" }] })), { tone: "quiet", icon: "goal", words: "runs on a goal", live: false });
  assert.equal(cardStatus(row({ problems: [{ kind: "no_start" }], workspace_problems: [{ kind: "needs_goal" }] })).words, "1 problem", "a problem outranks the reason it runs on a goal only — and one problem is said as one");
  const inWorkspace = row({ problems: [{ kind: "no_start" }], runs: { live: 2, total: 3 } });
  assert.deepEqual(cardStatus(inWorkspace), { tone: "accent", icon: "run", words: "running 2 runs", live: true }, "a run in motion outranks the draft's problem");
  assert.equal(cardStatus(row({ runs: { live: 1, total: 1 } })).words, "running one run");
  const running = row({ problems: [{ kind: "no_start" }], used_by: [goal("g1", true), goal("g2", false)] });
  assert.deepEqual(cardStatus(running), { tone: "accent", icon: "run", words: "running in one goal", live: true }, "a goal's run in motion too");
  assert.equal(cardStatus(row({ used_by: [goal("g1", true), goal("g2", true)] })).words, "running in 2 goals");
  assert.equal(cardStatus(row({ runs: { live: 1, total: 1 }, used_by: [goal("g1", true)] })).words, "running one run", "its own runs first");
  assert.deepEqual(cardStatus(row({ runs: { live: 1, total: 1 } }, { archived: { at: 1 } })), { tone: "quiet", icon: "archive", words: "archived", live: false }, "put away outranks everything");
});

test("the facts line: how big, where from", () => {
  assert.deepEqual(cardMeta(row()), ["3 steps", "yours"]);
  assert.deepEqual(cardMeta(row({}, { inputs: [{ name: "x" }, { name: "y" }], origin: { origin: "catalog", slug: "triage" } })), ["3 steps", "2 inputs", "from triage"]);
});

test("the ⋮: Open and Delete… always; Run… when it can start in the workspace; Stop and Restart while a run of it goes there", () => {
  const ids = (r) => cardMenu(workflowVerbs(r)).map((i) => i.id);
  const going = { runs: { live: 1, total: 1 } };
  assert.deepEqual(ids(row()), ["open", "run", "delete"]);
  assert.deepEqual(ids(row({ problems: [{ kind: "no_start" }] })), ["open", "delete"], "a problem: nothing starts, the card still opens and can go");
  assert.deepEqual(ids(row({ workspace_problems: [{ kind: "needs_goal" }] })), ["open", "delete"], "it runs on a goal only");
  assert.deepEqual(ids(row(going)), ["open", "run", "restart", "stop", "delete"]);
  assert.deepEqual(ids(row({ used_by: [goal("g1", true)] })), ["open", "run", "delete"], "a goal's run is its goal's to stop");
  assert.deepEqual(ids(row(going, { archived: { at: 1 } })), ["open", "stop", "delete"], "archived and running: its runs can stop, nothing restarts");
  const items = cardMenu(workflowVerbs(row(going)));
  assert.deepEqual(items.map((i) => [i.id, !!i.danger, !!i.separatorBefore]), [
    ["open", false, false],
    ["run", false, true],
    ["restart", false, false],
    ["stop", true, false],
    ["delete", true, true],
  ]);
  assert.equal(cardMenu(workflowVerbs(row({ runs: { live: 1, total: 1 } }, { archived: { at: 1 } })))[1].separatorBefore, true, "the first verb opens the group");
  assert.deepEqual(items.map((i) => i.label), ["Open", "Run…", "Restart every run", "Stop every run", "Delete…"]);
});

test("an On mark stands beside the state line while it listens — Paused when a failure stopped it", () => {
  assert.equal(onMark(row()), null, "Off: no mark");
  assert.deepEqual(onMark(row({ listening: { since: 1 } })), { words: "On", tone: "ok" });
  assert.deepEqual(onMark(row({ listening: { since: 1, paused: { reason: { reason: "run_failed", run: "01R" }, at: 2 } } })), { words: "Paused", tone: "warn" });
  assert.equal(onMark(null), null);
  assert.equal(cardStatus(row({ listening: { since: 1 } })).words, "ready to run", "the mark is beside the one state, never instead of it");
});

test("the ⋮ turns a workflow with an event start On, or Off once it is; one only events begin offers a test run", () => {
  const ids = (r) => cardMenu(workflowVerbs(r)).map((i) => i.id);
  assert.deepEqual(ids(row({ starts: [byHand, schedule] })), ["open", "run", "turn_on", "delete"]);
  assert.deepEqual(ids(row({ starts: [byHand, schedule], listening: { since: 1 } })), ["open", "run", "turn_off", "delete"]);
  assert.deepEqual(ids(row({ starts: [byHand] })), ["open", "run", "delete"], "nothing to hear: nothing to turn on");
  const eventOnly = cardMenu(workflowVerbs(row({ starts: [schedule], event_only: true })));
  assert.deepEqual(eventOnly.map((i) => [i.id, i.label, !!i.separatorBefore]), [
    ["open", "Open", false],
    ["run", "Test run…", true],
    ["turn_on", "Turn on…", false],
    ["delete", "Delete…", true],
  ]);
  assert.deepEqual(ids(row({ starts: [byHand, schedule], problems: [{ kind: "bad_timer" }] })), ["open", "delete"], "problems: neither runs nor turns on");
});

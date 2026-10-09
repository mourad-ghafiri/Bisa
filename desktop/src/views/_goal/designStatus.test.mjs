/**
 * The designing card's facts, tested where they live: what the page says
 * while the Workflow Agent owes a workflow, and which moves it offers.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { readFileSync } from "node:fs";

import { DESIGNING_KINDS, activityFrom, applyGuidedFrame, designInProgress, designView, kindWord, ticks } from "./designStatus.mjs";

const goal = { id: "01G", mode: "guided", closed: null };
const auto = { ...goal, mode: "auto" };
const guidance = (design, design_enabled = true) => ({ mode: "guided", design_enabled, phase: "design", design, open_questions: [] });
const at = (status, extra = {}) => ({ phase: "design", status, since: 1000, detail: null, session: null, live: status === "working", ...extra });

test("every kind has a headline and never contradicts itself", () => {
  const views = {
    scheduled: designView(guidance(at("scheduled")), goal, { now: 1010 }),
    working: designView(guidance(at("working")), goal, { now: 1010 }),
    asking: designView(guidance(at("asking", { detail: "Which tone?" })), goal, { now: 1010 }),
    proposed: designView(guidance(at("proposed", { detail: "Ship it (3 steps)" })), goal),
    stalled: designView(guidance(at("stalled", { detail: "no proposal after 600s" })), goal),
    failed: designView(guidance(at("failed", { detail: "cannot launch claude-code." })), goal),
    off: designView(guidance(at("off")), goal),
    manual: designView(guidance(null), { ...goal, mode: "manual" }),
  };
  for (const [kind, v] of Object.entries(views)) {
    assert.equal(v.kind, kind);
    assert.ok(v.headline.length > 8, `${kind} has a headline`);
  }
});

test("while the agent is at work, picking a workflow is not offered at all", () => {
  assert.deepEqual([...DESIGNING_KINDS], ["scheduled", "working", "asking"]);
  for (const status of DESIGNING_KINDS) {
    const v = designView(guidance(at(status)), goal, { now: 1010 });
    assert.equal(v.offerPick, false, `${status} offers no pick`);
    assert.notEqual(v.primary, "pick", status);
    assert.equal(v.offerRetry, false, status);
    assert.ok(v.elapsed, `${status} shows how long`);
    assert.doesNotMatch(v.headline, /No workflow yet/, status);
    assert.equal(designInProgress(guidance(at(status)), goal), true, `${status} is in progress`);
  }
  assert.equal(designInProgress(guidance(null), goal), true, "no fact yet on an enabled node: about to start");
  assert.equal(designView(guidance(at("proposed", { detail: "x" })), goal).offerPick, false, "a proposal is reviewed, not replaced");
  assert.equal(designInProgress(guidance(at("proposed")), goal), false, "a proposal is not work in progress");
  assert.match(designView(guidance(at("working")), goal).headline, /designing/);
  assert.match(designView(guidance(at("working", { phase: "repair" })), goal).headline, /repairing/);
  assert.match(designView(guidance(at("asking", { detail: "Which tone?" })), goal).hint, /Which tone\?/);
});

test("a stall or a failure offers a retry and says why; a proposal points at the review", () => {
  const stalled = designView(guidance(at("stalled", { detail: "no proposal after 600s" })), goal);
  assert.equal(stalled.primary, "retry");
  assert.equal(stalled.offerRetry, true);
  assert.equal(stalled.offerPick, true, "after a stall a person may pick by hand");
  assert.equal(designInProgress(guidance(at("stalled")), goal), false);
  assert.equal(stalled.tone, "danger");
  assert.match(stalled.hint, /No proposal after 600s/);
  const failed = designView(guidance(at("failed", { detail: "the session refused the prompt: boom." })), goal);
  assert.equal(failed.primary, "retry");
  assert.equal(failed.offerPick, true);
  assert.match(failed.hint, /refused the prompt/);
  const proposed = designView(guidance(at("proposed", { detail: "Ship it (3 steps)" })), goal);
  assert.equal(proposed.primary, "review");
  assert.equal(proposed.offerPick, false);
  assert.match(proposed.hint, /Ship it \(3 steps\)/);
});

test("designing off, a manual goal, and no fact yet each say the right thing", () => {
  const off = designView(guidance(at("off")), goal);
  assert.equal(off.kind, "off");
  assert.equal(off.primary, "pick");
  assert.equal(off.offerPick, true);
  assert.equal(designInProgress(guidance(at("off")), goal), false);
  assert.equal(off.offerAsk, false);
  assert.equal(designView(guidance(null, false), goal).kind, "off", "no fact and designing disabled");
  assert.match(designView(guidance(null, false), goal).headline, /Designing is off/);
  const fresh = designView(guidance(null), goal);
  assert.equal(fresh.kind, "scheduled", "no fact yet on an enabled node: it is about to start");
  const manual = designView(guidance(null), { ...goal, mode: "manual" });
  assert.equal(manual.kind, "manual");
  assert.equal(manual.primary, "design", "a manual goal's first move is the designer");
  assert.equal(manual.offerPick, true, "picking one from the library stays possible");
  assert.equal(designInProgress(guidance(null), { ...goal, mode: "manual" }), false, "a manual goal is designed by hand, before anything");
  assert.match(manual.headline, /Design how it runs/);
  assert.match(manual.hint, /Workflow tab/);
  assert.equal(designView(guidance(at("working")), { ...goal, closed: { reason: "abandoned" } }).offerAsk, false);
  assert.match(designView(guidance(at("scheduled")), goal, { paused: true }).headline, /paused/);
});

test("elapsed is said the platform's one way — seconds, minutes, hours — and never goes negative", () => {
  const elapsed = (now) => designView(guidance(at("working")), goal, { now }).elapsed;
  assert.equal(elapsed(1012), "12s");
  assert.equal(elapsed(1000 + 185), "3m 5s");
  assert.equal(elapsed(1000 + 3600 + 12 * 60), "1h 12m");
  assert.equal(elapsed(990), "0s");
  // A unit's letter is the catalog's: the model builds no span of its own.
  const model = readFileSync(new URL("./designStatus.mjs", import.meta.url), "utf8");
  assert.ok(model.includes("durationPrecise(") && !/`\$\{[a-z]+\}[smh]\b/.test(model));
});

test("with designing off, the hint is one sentence of the catalog — no letter is lowered in code to join two", () => {
  const off = designView(guidance(at("off")), goal);
  assert.equal(off.hint, "Nobody will design this goal's workflow. Draw it on the Workflow tab, or pick a workflow yourself instead — from the library or a template.");
  const model = readFileSync(new URL("./designStatus.mjs", import.meta.url), "utf8");
  assert.ok(!model.includes("toLowerCase()"));
});

test("a guided frame moves the card before the page refetches", () => {
  const prev = at("scheduled");
  const next = applyGuidedFrame(prev, { type: "guided", phase: "design", status: "working", detail: "on claude-code", session: "01S" }, 1200);
  assert.deepEqual(next, { phase: "design", status: "working", since: 1200, detail: "on claude-code", session: "01S", live: true });
  const stalled = applyGuidedFrame(next, { type: "guided", phase: "design", status: "stalled", detail: "timed out" }, 1300);
  assert.equal(stalled.live, false);
  assert.equal(stalled.session, "01S", "the session is remembered when the frame omits it");
  assert.equal(applyGuidedFrame(prev, { type: "step_changed" }, 1400), prev, "another frame is not a design change");
});

test("the activity line follows session frames and ignores noise", () => {
  const started = activityFrom({ type: "session", event: { tier: "lifecycle", event: { type: "started" } } }, null);
  assert.equal(started.text, "session started");
  const tool = activityFrom(
    { type: "session", event: { tier: "progress", event: { type: "tool_started", name: "get_goal", args_summary: "01G" } } },
    started,
  );
  assert.match(tool.text, /get_goal/);
  assert.equal(activityFrom({ type: "session", event: { tier: "raw", event: {} } }, tool), tool, "a raw frame keeps the last line");
  assert.equal(activityFrom({ type: "gate_opened" }, tool), tool, "another payload leaves it alone");
});

test("an auto goal's card says the agent starts the run; a guided one that you adopt", () => {
  const working = designView(guidance(at("working")), auto, { now: 1010 });
  assert.equal(working.kind, "working");
  assert.match(working.hint, /starts it/);
  assert.doesNotMatch(working.hint, /adopt/);
  assert.match(designView(guidance(null), auto).hint, /decides for itself/, "no round of questions in auto");
  const guided = designView(guidance(at("working")), goal, { now: 1010 });
  assert.match(guided.hint, /adopt it/);
  assert.match(guided.hint, /one round of questions/);
  assert.equal(designInProgress(guidance(at("working")), auto), true, "nobody picks over the agent's head, auto or guided");
});

test("the chip says the kind in the catalog's word, and the clock ticks exactly while the agent is at work or about to be", () => {
  const kinds = ["scheduled", "working", "asking", "proposed", "stalled", "failed", "off", "manual"];
  assert.deepEqual(kinds.map(kindWord), kinds, "in English the word is the kind");
  assert.equal(kindWord("resting"), "resting", "a kind this build has no word for is said as it came");
  assert.deepEqual(kinds.filter(ticks), [...DESIGNING_KINDS]);
  const card = readFileSync(new URL("./DesigningCard.tsx", import.meta.url), "utf8");
  assert.ok(card.includes("{kindWord(v.kind)}") && !card.includes(">{v.kind}<"), "the card spells no kind");
  assert.ok(card.includes("const live = ticks(v.kind);") && !card.includes('v.kind === "asking"'), "nor repeats which kinds are at work");
});

// added by the coverage pass: designStatus.test.mjs
test("a design status this build has no words for is shown as the agent at work, in the node's word", () => {
  const view = designView({ design: { status: "weird" } }, { mode: "guided" }, { now: 100 });
  assert.equal(view.kind, "working");
  assert.equal(view.tone, "quiet");
  assert.equal(view.hint, "");
  assert.ok(view.headline.includes("weird"), view.headline);
});

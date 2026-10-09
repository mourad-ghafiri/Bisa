/**
 * A step's boundary events as the designer edits and draws them: a fresh
 * one per event, a reminder that never diverts, an act change that takes a
 * divert's path away, a rename that carries it, a removal that drops it,
 * where each divert's handle sits, and the words a chip wears. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/forms/boundaryModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  DAY_SECS,
  DEFAULT_REMINDERS,
  OUT_WITH_DIVERTS,
  actsFor,
  addBoundary,
  blankAct,
  blankBoundary,
  blankOn,
  boundaryOf,
  chipOf,
  consequence,
  divertOffsets,
  freshBoundaryName,
  mayCarryBoundaries,
  ownOffsets,
  removeBoundaryOn,
  renameBoundaryOn,
  replaceBoundary,
  setAct,
  setEvent,
} from "./boundaryModel.mjs";
import { BOUNDARY_EVENTS } from "../stepKinds.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORE = join(HERE, "../../../../../crates/bisa-core/src");

const review = (over = {}) => ({
  id: "review",
  name: "Review",
  kind: "approval",
  prompt: "Ship it?",
  then: [{ to: "ship" }, { to: "escalate", branch: "late" }],
  boundaries: [
    { name: "late", on: { event: "after", secs: 172800 }, act: "divert" },
    { name: "nudge", on: { event: "every", secs: DAY_SECS, max: 5 }, act: "notify", template: "Still waiting on your decision." },
  ],
  ...over,
});

test("a reminder fires the core's default number of times, and never diverts", () => {
  const src = readFileSync(join(CORE, "boundary.rs"), "utf8");
  const m = src.match(/pub const DEFAULT_REMINDERS: u32 = (\d+);/);
  assert.ok(m, "the core names its default");
  assert.equal(DEFAULT_REMINDERS, Number(m[1]));
  assert.deepEqual(actsFor("every"), ["notify", "emit"], "a divert on a cadence would stop the step at its first tick");
  for (const event of ["after", "message", "signal"]) assert.deepEqual(actsFor(event), ["divert", "notify", "emit"]);
});

test("a fresh boundary of every event: a timeout, a message and a signal divert, a reminder posts — each under a name the step does not use", () => {
  for (const { event } of BOUNDARY_EVENTS) {
    const b = blankBoundary(review(), event);
    assert.equal(b.on.event, event);
    assert.ok(actsFor(event).includes(b.act), `${event} takes an act it may`);
  }
  assert.deepEqual(blankBoundary(review(), "after"), { name: "timeout", on: { event: "after", secs: DAY_SECS }, act: "divert" });
  assert.deepEqual(blankBoundary(review(), "every"), { name: "reminder", on: { event: "every", secs: DAY_SECS, max: DEFAULT_REMINDERS }, act: "notify", template: "" });
  assert.deepEqual(blankOn("signal"), { event: "signal", name: "" });
  assert.deepEqual(blankAct("emit"), { act: "emit", signal: "" });
  assert.throws(() => blankOn("forever"), /not a boundary event/);
  assert.throws(() => blankAct("escalate"), /not a boundary act/);
  const twice = addBoundary(addBoundary(review(), "after"), "after");
  assert.deepEqual(twice.boundaries.map((b) => b.name), ["late", "nudge", "timeout", "timeout-2"]);
  assert.equal(freshBoundaryName(review(), "late"), "late-2", "a name another boundary has is taken");
  assert.equal(freshBoundaryName({ kind: "agent" }, "timeout"), "timeout");
});

test("only a step whose work can be stopped carries one", () => {
  assert.ok(mayCarryBoundaries(review()));
  assert.ok(mayCarryBoundaries({ kind: "spawn", wait: true }));
  assert.ok(!mayCarryBoundaries({ kind: "spawn", wait: false }));
  for (const kind of ["start", "check", "connector", "judge", "decide", "parallel", "emit", "end", "notify"]) assert.ok(!mayCarryBoundaries({ kind }), kind);
});

test("an act changed from a divert takes the divert's path away; a reminder asked to divert keeps its act", () => {
  const posted = setAct(review(), "late", "notify");
  assert.deepEqual(boundaryOf(posted, "late"), { name: "late", on: { event: "after", secs: 172800 }, act: "notify", template: "" });
  assert.deepEqual(posted.then, [{ to: "ship" }], "nothing takes a path an act never opens");
  const diverting = setAct(posted, "late", "divert");
  assert.equal(boundaryOf(diverting, "late").act, "divert");
  assert.deepEqual(diverting.then, [{ to: "ship" }], "a new divert has no flow until one is drawn");
  const s = review();
  assert.equal(setAct(s, "nudge", "divert"), s, "a reminder never diverts");
  assert.equal(setAct(s, "late", "divert"), s, "the same act is no edit");
  assert.equal(setAct(s, "nope", "notify"), s, "a boundary the step does not have is no edit");
});

test("an event changed: a fresh clock or filter, the act kept when it may be — a divert turned reminder posts and loses its path", () => {
  const toMessage = setEvent(review(), "late", "message");
  assert.deepEqual(boundaryOf(toMessage, "late"), { name: "late", on: { event: "message" }, act: "divert" });
  assert.ok(toMessage.then.some((f) => f.branch === "late"), "a message still diverts along the same path");
  const toReminder = setEvent(review(), "late", "every");
  assert.equal(boundaryOf(toReminder, "late").act, "notify");
  assert.deepEqual(toReminder.then, [{ to: "ship" }]);
  const kept = setEvent(review(), "nudge", "signal");
  assert.deepEqual(boundaryOf(kept, "nudge"), { name: "nudge", on: { event: "signal", name: "" }, act: "notify", template: "Still waiting on your decision." }, "the post's words ride along");
  const s = review();
  assert.equal(setEvent(s, "late", "after"), s, "the same event is no edit");
});

test("a rename carries the divert's flows, and refuses an empty, a long or a taken name", () => {
  const r = renameBoundaryOn(review(), "late", "overdue");
  assert.equal(r.ok, true);
  assert.deepEqual(r.step.boundaries.map((b) => b.name), ["overdue", "nudge"]);
  assert.deepEqual(r.step.then, [{ to: "ship" }, { to: "escalate", branch: "overdue" }], "the path follows its name");
  assert.equal(renameBoundaryOn(review(), "late", "late").step.then.length, 2, "the same name is no edit");
  assert.equal(renameBoundaryOn(review(), "late", "  ").ok, false);
  assert.equal(renameBoundaryOn(review(), "late", "x".repeat(65)).ok, false);
  const taken = renameBoundaryOn(review(), "late", "nudge");
  assert.equal(taken.ok, false);
  assert.match(taken.reason, /nudge/);
  assert.equal(renameBoundaryOn(review(), "nope", "x").ok, false);
});

test("a removal takes a divert's path with it, and an act leaves the flows alone", () => {
  const gone = removeBoundaryOn(review(), "late");
  assert.deepEqual(gone.boundaries.map((b) => b.name), ["nudge"]);
  assert.deepEqual(gone.then, [{ to: "ship" }]);
  const quiet = removeBoundaryOn(review(), "nudge");
  assert.equal(quiet.then.length, 2);
  const s = review();
  assert.equal(removeBoundaryOn(s, "nope"), s);
  const edited = replaceBoundary(review(), "nudge", { name: "renamed", on: { event: "every", secs: 60, max: 2 }, act: "notify", template: "Hurry." });
  assert.equal(boundaryOf(edited, "nudge").template, "Hurry.", "a field's edit never renames");
});

test("a divert's handle leaves the card's lower right, the normal flow keeps the left, and a gateway spreads its branches", () => {
  assert.deepEqual([...divertOffsets(review()).entries()], [["late", 75]]);
  const two = review({ boundaries: [...review().boundaries, { name: "cancelled", on: { event: "message" }, act: "divert" }] });
  assert.deepEqual([...divertOffsets(two).entries()], [["late", 55], ["cancelled", 95]]);
  assert.deepEqual(ownOffsets(review()), { out: OUT_WITH_DIVERTS, branches: new Map() });
  assert.deepEqual(ownOffsets({ kind: "agent" }), { out: 50, branches: new Map() });
  const gate = ownOffsets({ kind: "if", when: { condition: "all", of: [] } });
  assert.equal(gate.out, null);
  assert.deepEqual([...gate.branches.entries()].map(([b, x]) => [b, Math.round(x)]), [["yes", 33], ["no", 67]]);
  assert.equal(divertOffsets(null).size, 0);
});

test("a chip says its clock or filter in a few words, and whether it diverts", () => {
  assert.deepEqual(chipOf(review().boundaries[0]), { name: "late", event: "after", act: "divert", diverts: true, words: "after 2d" });
  const nudge = chipOf(review().boundaries[1]);
  assert.equal(nudge.diverts, false);
  assert.equal(nudge.words, "every 1d · up to 5");
  assert.equal(chipOf({ name: "t", on: { event: "after", secs: { input: "patience" } }, act: "divert" }).words, "after {inputs.patience}");
  assert.equal(chipOf({ name: "m", on: { event: "message" }, act: "divert" }).words, "message");
  assert.equal(chipOf({ name: "m", on: { event: "message", contains: "cancel" }, act: "divert" }).words, "message “cancel”");
  assert.equal(chipOf({ name: "s", on: { event: "signal", name: "deploy.started" }, act: "emit", signal: "x" }).words, "signal deploy.started");
  assert.equal(chipOf({ name: "r", on: { event: "every", secs: 3600 }, act: "notify", template: "" }).words, `every 1h · up to ${DEFAULT_REMINDERS}`, "an unwritten max is the core's default");
});

test("under each boundary, the editor says where a divert leads or what an act does beside the step", () => {
  const s = review();
  assert.equal(consequence(s, s.boundaries[0]), "Diverts to escalate.");
  assert.equal(consequence({ ...s, then: [{ to: "ship" }] }, s.boundaries[0]), "Diverts nowhere yet: draw a flow from its handle — it carries “late”.");
  assert.equal(consequence(s, s.boundaries[1]), "Posts beside the live step; the step goes on.");
  assert.equal(consequence(s, { name: "e", on: { event: "after", secs: 5 }, act: "emit", signal: "x" }), "Raises its signal beside the live step; the step goes on.");
});

// added by the coverage pass: boundaryModel.test.mjs
test("an emit act keeps its signal and only a payload with keys, and an unknown event reads as its own word", () => {
  const emitting = {
    id: "s",
    name: "s",
    kind: "agent",
    boundaries: [{ name: "late", on: { event: "after", secs: 60 }, act: "emit", signal: "late.again", payload: { why: "slow" } }],
    then: [],
  };
  const moved = setEvent(emitting, "late", "message");
  assert.deepEqual(moved.boundaries[0].on, blankOn("message"));
  assert.equal(moved.boundaries[0].act, "emit");
  assert.equal(moved.boundaries[0].signal, "late.again");
  assert.deepEqual(moved.boundaries[0].payload, { why: "slow" });
  const empty = setEvent({ ...emitting, boundaries: [{ ...emitting.boundaries[0], payload: {} }] }, "late", "message");
  assert.equal("payload" in empty.boundaries[0], false, "an empty payload is not written");
  assert.equal(chipOf({ name: "odd", on: { event: "weird" }, act: "divert" }).words, "weird");
});

/**
 * What a host's events hold for a person to read (guide/events §Durable,
 * deduplicated, and never in a loop): the held signals of one host, oldest
 * first, in words, with the one verb that lets one through. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/heldSignalsModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { HELD_PAGE, heldOf, heldWords, hostOf, movesSignalsOf, signalsQuery } from "./heldSignalsModel.mjs";

const signal = (id, state, over = {}) => ({ id, listener: "workspace:01WF/ticket", source: "hook", at: 100, state, scope: { scope: "workspace" }, ...over });

test("a host is a library workflow or a goal, as the node names it", () => {
  assert.equal(hostOf({ workflow: "01WF" }), "workspace:01WF");
  assert.equal(hostOf({ goal: "01G" }), "goal:01G");
  assert.equal(hostOf({ goal: "01G", workflow: "01WF" }), "goal:01G", "a goal's own design listens through its goal");
  assert.equal(hostOf({}), null);
  assert.equal(hostOf(null), null);
  // The node's own spelling (`ListenerHost`).
  const core = readFileSync(new URL("../../../../crates/bisa-core/src/listen.rs", import.meta.url), "utf8");
  assert.ok(core.includes("Wire form `workspace:<WorkflowId>`") && core.includes("`goal:<GoalId>`"), "the wire form of a host");
  assert.ok(core.includes('ListenerHost::Workspace { .. } => "workspace"') && core.includes('ListenerHost::Goal { .. } => "goal"'));
});

test("the signals asked of the node are one host's, a page of them", () => {
  assert.deepEqual(signalsQuery("workspace:01WF"), { host: "workspace:01WF", limit: HELD_PAGE });
  assert.ok(HELD_PAGE > 0 && HELD_PAGE <= 200, "the node answers 200 at most");
});

test("what waits on a person is what is held for this host — the oldest first, nothing another host's, nothing that moved on", () => {
  const signals = [
    signal("s5", "done", { at: 500 }),
    signal("s4", "held", { at: 400, source: "connector", listener: "workspace:01WF/issues", note: "held by the content screen — no verdict; let it through or leave it" }),
    signal("s3", "held", { at: 300, listener: "workspace:01OTHER/ticket", note: "held by the content screen — asks for a token; let it through or leave it" }),
    signal("s2", "held", { at: 200, note: "held by the content screen — asks for a token; let it through or leave it" }),
    signal("s1", "skipped", { at: 100, note: "the guard dropped it" }),
    signal("s0", "held", { at: 50, listener: null, source: "signal", name: "deploy.finished" }),
  ];
  const held = heldOf(signals, "workspace:01WF");
  assert.deepEqual(held.map((h) => [h.id, h.step, h.at]), [["s2", "ticket", 200], ["s4", "issues", 400]]);
  assert.equal(held[0].source, "a call to its hook");
  assert.equal(held[1].source, "an item its poll listed");
  assert.equal(held[0].why, "held by the content screen — asks for a token; let it through or leave it", "the node's own reason, whole");
  assert.deepEqual(heldOf(signals, "workspace:01W"), [], "a host whose id merely starts the same is another host");
  assert.deepEqual(heldOf(null, "workspace:01WF"), []);
  assert.deepEqual(heldOf(signals, null), []);
  assert.equal(heldOf([signal("s9", "held", { source: "carrier_pigeon" })], "workspace:01WF")[0].source, "an event", "a source this build has no word for");
  assert.equal(heldOf([signal("s9", "held", { source: "signal", name: "deploy.finished" })], "workspace:01WF")[0].source, "the signal deploy.finished");
  assert.equal(heldOf([signal("s9", "held")], "workspace:01WF")[0].why, "held for you to read", "a held signal that says no more");
});

test("the chip counts what is held, and says nothing when nothing is", () => {
  assert.equal(heldWords(0), null);
  assert.equal(heldWords(1), "1 held for you");
  assert.equal(heldWords(3), "3 held for you");
});

test("the list is read again when an event of this host is written down, begins its run, or is refused", () => {
  const about = (type, listener) => ({ payload: { type, listener, signal: "s1" } });
  for (const type of ["signal_received", "listener_fired", "listener_failed"]) assert.ok(movesSignalsOf(about(type, "workspace:01WF/ticket"), "workspace:01WF"), type);
  assert.ok(!movesSignalsOf(about("signal_received", "workspace:01WFX/ticket"), "workspace:01WF"));
  assert.ok(!movesSignalsOf(about("signal_received", null), "workspace:01WF"), "a named signal kept for the waits is no listener's");
  assert.ok(!movesSignalsOf({ payload: { type: "listening_changed", host: "workspace:01WF", on: true } }, "workspace:01WF"), "turning On moves the switch, not what is held");
  assert.ok(!movesSignalsOf(null, "workspace:01WF"));
  assert.ok(!movesSignalsOf(about("signal_received", "workspace:01WF/ticket"), null));
});

/**
 * The node's boot and its failures, as the shell says them.
 * Run with `node --test desktop/src/shell/nodeBootModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { NODE_BOOT_INITIAL, NODE_DOORS, NODE_EVENTS, bootLine, doorFailedWords, doorWords, failureWords, nodeDoors, phaseWords, reduceNodeEvent, seedFromStatus } from "./nodeBootModel.mjs";

const boot = (state, payload) => reduceNodeEvent(state, NODE_EVENTS.boot, payload);
const failed = (state, payload) => reduceNodeEvent(state, NODE_EVENTS.failed, payload);

test("nothing heard is nothing said: the plain offline words stand", () => {
  assert.equal(bootLine(NODE_BOOT_INITIAL), null);
  assert.equal(bootLine(null), null);
  assert.equal(NODE_BOOT_INITIAL.known, false);
});

test("the boot's phases read in order, the rebuild with its count, and ready ends them", () => {
  let s = boot(NODE_BOOT_INITIAL, { phase: "opening_workspace" });
  assert.equal(bootLine(s), "Opening the workspace…");
  assert.equal(s.known, true);
  s = boot(s, { phase: "rebuilding_index", done: 0, of: 300 });
  assert.equal(bootLine(s), "Rebuilding the index — 0 of 300 goals…");
  s = boot(s, { phase: "rebuilding_index", done: 25, of: 300 });
  assert.equal(bootLine(s), "Rebuilding the index — 25 of 300 goals…");
  assert.equal(phaseWords("rebuilding_index", null, null), "Rebuilding the index…", "no count yet: the phase alone");
  assert.equal(phaseWords("rebuilding_index", 0, 0), "Rebuilding the index…", "nothing to rebuild: no count");
  s = boot(s, { phase: "starting_engine" });
  assert.equal(bootLine(s), "Starting the engine…");
  s = boot(s, { phase: "ready" });
  assert.equal(s.ready, true);
  assert.equal(bootLine(s), null, "the node answers: nothing to say");
  assert.equal(phaseWords("a_phase_this_build_does_not_know", null, null), null, "a newer shell's word costs nothing");
  assert.equal(boot(s, { nope: true }), s, "a boot word without a phase changes nothing");
  assert.equal(reduceNodeEvent(s, "node:something-else", {}), s, "another event changes nothing");
});

test("a failure says why and when the next try comes, and what the node said is the detail", () => {
  const exited = failed(NODE_BOOT_INITIAL, { kind: "exited", how: "exited with code 1", said: ["refused the workspace", "owner key unreadable"], attempt: 2, next_in_secs: 4 });
  assert.equal(bootLine(exited), "The node exited with code 1 before it answered — trying again in 4 s");
  assert.deepEqual(failureWords(exited.failure), { line: "The node exited with code 1 before it answered", details: "It said: refused the workspace · owner key unreadable" });
  assert.equal(exited.attempt, 2);
  assert.equal(exited.ready, false);
  const silent = failed(NODE_BOOT_INITIAL, { kind: "timed_out", secs: 190, said: [], attempt: 1, next_in_secs: 2 });
  assert.equal(bootLine(silent), "The node did not answer within 190 s — trying again in 2 s");
  assert.equal(failureWords(silent.failure).details, null, "nothing said: no fold");
  const none = failed(NODE_BOOT_INITIAL, { kind: "no_binary", tried: ["/opt/bisa: not found", "bisa: not found"], attempt: 0, next_in_secs: 1 });
  assert.equal(bootLine(none), "No bisa node could be started on this machine — trying again in 1 s");
  assert.equal(failureWords(none.failure).details, "/opt/bisa: not found\nbisa: not found");
  const held = failed(NODE_BOOT_INITIAL, { kind: "held_by_other", pid: 4242, attempt: 0, next_in_secs: 5 });
  assert.equal(bootLine(held), "Another node holds this workspace (pid 4242); Bisa starts its own when it stops — trying again in 5 s");
  const noNext = failed(NODE_BOOT_INITIAL, { kind: "exited", how: "was ended by signal 9", said: [], attempt: 1 });
  assert.equal(bootLine(noNext), "The node was ended by signal 9 before it answered", "no next try known: the line alone");
  const odd = failed(NODE_BOOT_INITIAL, { kind: 7, how: 3, said: "x", attempt: "2", next_in_secs: "soon" });
  assert.deepEqual(odd.failure, { kind: "exited", how: null, said: [], secs: null, pid: null, tried: [] }, "a payload of another shape is read as far as it goes");
  assert.equal(odd.attempt, 0);
  assert.equal(odd.nextInSecs, null);
});

test("a boot word after a failure clears it, and a restart is ready", () => {
  const down = failed(NODE_BOOT_INITIAL, { kind: "exited", how: "exited with code 1", said: [], attempt: 1, next_in_secs: 2 });
  const again = boot(down, { phase: "opening_workspace" });
  assert.equal(again.failure, null);
  assert.equal(again.nextInSecs, null);
  assert.equal(bootLine(again), "Opening the workspace…");
  const up = reduceNodeEvent(again, NODE_EVENTS.restarted, { port: 4477, attempt: 1, requested: false });
  assert.equal(up.ready, true);
  assert.equal(up.phase, "ready");
  assert.equal(bootLine(up), null);
});

test("the seed from the shell's status is read only before anything was heard", () => {
  const booting = seedFromStatus(NODE_BOOT_INITIAL, { running: false, external: false, booting: true, restarts: 0, failure: null });
  assert.equal(bootLine(booting), "Starting the node…");
  const up = seedFromStatus(NODE_BOOT_INITIAL, { running: true, external: false, booting: false, restarts: 0, failure: null });
  assert.equal(up.ready, true);
  const external = seedFromStatus(NODE_BOOT_INITIAL, { running: true, external: true, restarts: 0 });
  assert.equal(external.ready, true, "a person's own node is a node");
  const down = seedFromStatus(NODE_BOOT_INITIAL, { running: false, external: false, booting: false, restarts: 3, failure: { kind: "held_by_other", pid: 9 } });
  assert.equal(bootLine(down), "Another node holds this workspace (pid 9); Bisa starts its own when it stops");
  assert.equal(down.attempt, 3);
  const heard = boot(NODE_BOOT_INITIAL, { phase: "ready" });
  assert.equal(seedFromStatus(heard, { running: false, external: false, booting: true, restarts: 0 }), heard, "an event that came first is newer than the answer");
  assert.equal(seedFromStatus(NODE_BOOT_INITIAL, null), NODE_BOOT_INITIAL, "off the shell: nothing");
  const idle = seedFromStatus(NODE_BOOT_INITIAL, { running: false, external: false, booting: false, restarts: 0, failure: null });
  assert.equal(idle, NODE_BOOT_INITIAL, "no child and no failure yet: nothing known");
});

test("the doors beside a node that is away: restart, the log, the data folder, the way out — no restart of somebody else's node", () => {
  assert.deepEqual(nodeDoors(NODE_BOOT_INITIAL), ["restart_now", "reveal_log", "open_data_folder", "quit"]);
  assert.deepEqual([...NODE_DOORS], nodeDoors(null));
  const held = failed(NODE_BOOT_INITIAL, { kind: "held_by_other", pid: 1, attempt: 0, next_in_secs: 5 });
  assert.deepEqual(nodeDoors(held), ["reveal_log", "open_data_folder", "quit"], "that node is not this desktop's to restart");
  assert.deepEqual(NODE_DOORS.map(doorWords), ["Restart now", "Reveal the log", "Open the data folder", "Quit Bisa"]);
  assert.equal(doorWords("a_door_this_build_does_not_know"), "a_door_this_build_does_not_know");
  assert.equal(doorFailedWords("reveal_log", new Error("no folder is known yet")), "Reveal the log did not work: no folder is known yet");
  assert.equal(doorFailedWords("quit", "refused"), "Quit Bisa did not work: refused");
});

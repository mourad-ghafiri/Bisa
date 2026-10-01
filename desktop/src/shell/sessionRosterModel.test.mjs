/**
 * The session roster's rules, tested where they live: a frame moves a row, a
 * read lands under the frames that outran it, and a Stop on a row the node
 * no longer has drops the row and fails nothing. No DOM.
 *
 * Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { dropped, landedRead, stopWords, stoppedAlready, upserted } from "./sessionRosterModel.mjs";
import { isStoppable } from "../ui/sessionState.mjs";

const row = (id, state, extra = {}) => ({ id, harness: "claude", kind: "engine", since: 1, state: { state }, children: [], ...extra });
const ids = (rows) => rows.map((r) => `${r.id}:${r.state.state}`);

test("a frame replaces the row where it stands, a new session leads, and a gone one leaves", () => {
  const roster = [row("s1", "running"), row("s2", "idle")];
  assert.deepEqual(ids(upserted(roster, row("s2", "thinking"))), ["s1:running", "s2:thinking"]);
  assert.deepEqual(ids(upserted(roster, row("s3", "starting"))), ["s3:starting", "s1:running", "s2:idle"]);
  assert.deepEqual(ids(dropped(roster, "s1")), ["s2:idle"]);
  assert.equal(dropped(roster, "nobody"), roster, "the same array: nothing repaints for a row the roster never held");
  assert.deepEqual(ids(roster), ["s1:running", "s2:idle"], "the roster handed in is left as it was");
});

test("a row that reads aborted stays aborted: a read asked for before the Stop and answered after it lands under the frame", () => {
  // The safety read goes out; the person presses Stop; the node says `aborted` on the bus; then the read answers, from before.
  const since = new Map([["s1", row("s1", "aborted")]]);
  const stale = [row("s1", "running"), row("s2", "idle")];
  assert.deepEqual(ids(landedRead(stale, since)), ["s1:aborted", "s2:idle"], "the older word never takes the row back to running — Stop would be offered again on a session that is over");
  assert.equal(isStoppable(landedRead(stale, since)[0].state), false);
  // A session that went while the read was out does not come back; one that started while it was out is there.
  const moved = new Map([["s2", null], ["s4", row("s4", "starting")]]);
  assert.deepEqual(ids(landedRead(stale, moved)), ["s4:starting", "s1:running"]);
  // Two that started meanwhile: the newest leads, as frames alone would have left them.
  const two = new Map([["s4", row("s4", "starting")], ["s5", row("s5", "starting")]]);
  assert.deepEqual(ids(landedRead([row("s1", "running")], two)), ["s5:starting", "s4:starting", "s1:running"]);
  // No frame meanwhile: the node's rows, as they came.
  assert.deepEqual(ids(landedRead(stale, new Map())), ["s1:running", "s2:idle"]);
});

test("a Stop on a row the node no longer has is no failure: the row is dropped and the toast says it had ended", () => {
  assert.equal(stoppedAlready({ status: 404, message: "no session 01J…" }), true);
  assert.equal(stoppedAlready({ status: 400, message: "not a session id" }), false);
  assert.equal(stoppedAlready({ status: 0, message: "the node did not answer" }), false, "no answer is not an answer of no such session");
  assert.equal(stoppedAlready(new Error("boom")), false);
  assert.equal(stoppedAlready(null), false);
  assert.equal(stopWords("stopped", "Session aborted"), "Session aborted");
  assert.equal(stopWords("gone", "Session aborted"), "That session had already ended.");
});

test("every Stop on the desktop goes through the one door, and the store holds a read to the frames that outran it", () => {
  const src = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  const store = src("./sessionsStore.ts");
  assert.ok(store.includes("if (!stoppedAlready(e)) throw e;") && store.includes('return "gone";'), "the node's 404 drops the row and throws nothing");
  assert.ok(store.includes("commit(landedRead(r.sessions, heard));"), "a read lands under the frames heard while it was out");
  assert.ok(store.includes("since?.set(row.id, row);") && store.includes("since?.set(id, null);"), "both kinds of frame are kept while a read is out");
  for (const surface of ["../views/Agents.tsx", "../views/_workbench/useConversationPane.tsx", "../views/_studio/ConversationThread.tsx", "../views/_workbench/ProjectRail.tsx", "../views/_work/useReviewRun.ts"]) {
    const text = src(surface);
    assert.ok(text.includes("stopSession("), `${surface} stops through the store`);
    assert.ok(!text.includes("api.abortSession("), `${surface} never calls the route itself: a 404 there was an error toast on every press`);
  }
});

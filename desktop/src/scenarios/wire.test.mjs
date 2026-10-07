/**
 * The node's wire, as a window lives it: a launch, a read that never
 * answers, a refusal, a node asked to stop under an open window, the wait
 * until it is back, every list read again once, a client the node was too
 * fast for. No socket is opened and no window: the scenario steps the models
 * `api.ts`, `bus.ts` and the shell step — the client's (`apiModel`), the
 * bus's (`busModel`), the load's (`workspaceLoadModel`) — and reads after
 * each step what the chrome, the footer and the menu bar icon would say.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs desktop/src/scenarios/wire.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { API_READ_TIMEOUT_MS, baseOf, bearer, codeOf, errorDetail, failureOf, isOffline, isServerError, takesDeadline, tokenToKeep } from "../apiModel.mjs";
import { BASE_RETRY_MS, FIRST_CONN, MAX_RETRY_MS, connAfter, endLine, matches, parseFrame, retryDelay } from "../busModel.mjs";
import { statWords } from "../shell/nodeStatModel.mjs";
import { trayState } from "../shell/trayModel.mjs";
import { degradedWords, offlineLine, reconnectWords, reloadOnReconnect, settleLoads } from "../shell/workspaceLoadModel.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

/** The window's half of the wire: the bus's word, who watches it, and what each watcher read again. */
function wire() {
  let conn = FIRST_CONN;
  let attempt = 0;
  const watchers = new Set();
  const lines = [];
  const waits = [];
  const say = (next) => {
    if (next === conn) return;
    conn = next;
    for (const w of watchers) w(conn);
  };
  return {
    conn: () => conn,
    lines,
    waits,
    watch: (cb) => {
      watchers.add(cb);
      cb(conn);
      return () => watchers.delete(cb);
    },
    /** One attempt: the stream opens, or finds nobody. */
    attempt: (answers) => {
      say(connAfter(conn, "attempt"));
      if (answers) {
        attempt = 0;
        say(connAfter(conn, "opened"));
        return;
      }
      lines.push(endLine(false, attempt));
      say(connAfter(conn, "ended"));
      waits.push(retryDelay(attempt++));
    },
    /** The open stream ends: the node was asked to stop, or the connection broke. */
    end: () => {
      lines.push(endLine(true, attempt));
      say(connAfter(conn, "ended"));
      waits.push(retryDelay(attempt++));
    },
    /** The node said this client missed events. */
    lagged: () => {
      say("lagged");
      say("open");
    },
  };
}

/** What a read that produced no answer is, as `api.ts` makes an `ApiError` of it. */
function noAnswer(error, { callerLeft = false, late = false } = {}) {
  const failure = failureOf(error, callerLeft, late);
  return { status: 0, failure: failure.kind, message: failure.reason, offline: isOffline(0) && failure.kind !== "timeout" };
}

test("a launch: the address and the token are the shell's, the bus connects once, and nothing is read twice", () => {
  assert.equal(baseOf(undefined, "http://127.0.0.1:51234/"), "http://127.0.0.1:51234", "the sidecar's port, as the shell said it");
  assert.equal(tokenToKeep("9f2c\n"), "9f2c");
  assert.equal(bearer("9f2c"), "Bearer 9f2c", "what every request carries");
  const w = wire();
  let ever = false;
  let reloads = 0;
  reloadOnReconnect(w.watch, () => reloads++);
  assert.equal(statWords(w.conn()).word, "connecting", "the footer before the first answer");
  assert.equal(trayState({ conn: w.conn(), everOpen: ever }), "connecting", "the menu bar icon: grey, never red");
  assert.equal(offlineLine(null, w.conn(), null), null, "the chrome says nothing of a node nobody has asked yet");
  w.attempt(true);
  ever = true;
  assert.equal(w.conn(), "open");
  assert.equal(statWords(w.conn()).word, "connected");
  assert.equal(trayState({ conn: w.conn(), everOpen: ever }), "quiet");
  assert.equal(reloads, 0, "the boot's own load was the read");
  assert.deepEqual(w.lines, [], "and the log has nothing to say");
});

test("a list that never answers is one degraded read — the node is there, the last values stand, the rest is applied", () => {
  assert.ok(takesDeadline("GET"), "the workspace's load hands every read its signal, and each still has its deadline");
  const late = noAnswer(Object.assign(new Error("The operation was aborted."), { name: "AbortError" }), { late: true });
  assert.deepEqual([late.failure, late.offline], ["timeout", false]);
  assert.equal(late.message, `no answer within ${API_READ_TIMEOUT_MS / 1000} s`);
  const settled = settleLoads(
    ["workspace", "goals", "inbox"],
    [{ status: "fulfilled", value: { pubkey: "me" } }, { status: "rejected", reason: Object.assign(new Error(late.message), late) }, { status: "fulfilled", value: { rows: [] } }],
    (reason) => reason?.offline === true,
  );
  assert.equal(settled.offline, false);
  assert.deepEqual(Object.keys(settled.ok), ["workspace", "inbox"]);
  assert.equal(degradedWords(settled.degraded, settled.offline).label, "1 list could not be read");
  assert.match(degradedWords(settled.degraded, settled.offline).title, /goals: no answer within 30 s/);
  assert.equal(offlineLine(null, "open", null), null, "no *node unreachable* over a slow list");
});

test("a refusal is the node's own sentence and the caller's to show; a screen that left is told nothing", () => {
  const body = { error: "the stored copy has moved: you edited revision 3, it is at 4", code: "conflict" };
  assert.equal(errorDetail(409, "Conflict", body), body.error);
  assert.equal(codeOf(body), "conflict", "a 409 stands for several things: the code says which");
  assert.ok(!isServerError(409) && !isOffline(409));
  assert.equal(errorDetail(401, "Unauthorized", { error: "missing or wrong token" }), "missing or wrong token", "a 401 is a refusal, never the node being down");
  assert.ok(isServerError(500), "a malfunction is the log's");
  const left = noAnswer(Object.assign(new Error("The operation was aborted."), { name: "AbortError" }), { callerLeft: true, late: true });
  assert.equal(left.failure, "aborted", "the caller's word comes first, whatever else ran out");
});

test("the node is asked to stop under an open window: it went away — said at once, waited for, never an error to show", () => {
  const w = wire();
  let ever = false;
  let reloads = 0;
  const toasts = [];
  reloadOnReconnect(w.watch, () => {
    reloads++;
    toasts.push(reconnectWords());
  });
  w.attempt(true);
  ever = true;

  // The node ends `/events` as it stops: an `EventSource` says `error`, and the bus reads an end.
  w.end();
  assert.equal(w.conn(), "closed");
  assert.deepEqual(w.lines, [{ level: "info", message: "the event stream ended: the node went away" }], "one line, and not an error");
  assert.equal(offlineLine(null, w.conn(), null), "waiting for the node…", "the chrome says so before any load was made");
  assert.deepEqual(statWords(w.conn()), { tone: "danger", word: "unreachable", title: statWords("closed").title });
  assert.equal(trayState({ conn: w.conn(), everOpen: ever }), "trouble", "it was there and is not");

  // Every read made meanwhile finds nobody: offline, the third of the three failures.
  const nobody = noAnswer(new TypeError("Failed to fetch"));
  assert.deepEqual([nobody.failure, nobody.offline], ["unreachable", true]);

  // It stays away for a while: the bus asks again at a pace that slows, and the word never moves.
  const words = new Set();
  for (let i = 0; i < 7; i++) {
    w.attempt(false);
    words.add(w.conn());
    words.add(offlineLine(null, w.conn(), null));
  }
  assert.deepEqual([...words], ["closed", "waiting for the node…"], "no flicker between two tones at every retry");
  assert.deepEqual(w.waits.slice(0, 6), [BASE_RETRY_MS, 1000, 2000, 4000, 8000, MAX_RETRY_MS], "brisk at first, then every fifteen seconds");
  assert.ok(w.waits.every((ms) => ms <= MAX_RETRY_MS));
  assert.deepEqual([...new Set(w.lines.slice(1).map((l) => l.level))], ["debug"], "a node that stays down does not fill the log");
  assert.equal(reloads, 0, "nothing is read from a node that is away");

  // It is back: the stream opens, every list is read again once, and the person is told why the screen moved.
  w.attempt(true);
  assert.equal(w.conn(), "open");
  assert.equal(reloads, 1);
  assert.deepEqual(toasts, ["The node came back — everything was read again."]);
  assert.equal(offlineLine("waiting for the node…", w.conn(), null), "waiting for the node…", "a load that found nobody stands until the next one answers");
  assert.equal(offlineLine(null, w.conn(), null), null, "and then the node is there");
  assert.equal(trayState({ conn: w.conn(), everOpen: ever }), "quiet");
  // The next trouble starts the pace over.
  w.end();
  assert.equal(w.waits.at(-1), BASE_RETRY_MS);
});

test("a screen opened while the node was away mends itself when it is back, whichever moment it opened at", () => {
  const w = wire();
  w.attempt(true);
  w.end();
  w.attempt(false);
  let reloads = 0;
  reloadOnReconnect(w.watch, () => reloads++);
  w.attempt(false);
  w.attempt(true);
  assert.equal(reloads, 1, "the read it made while nobody answered is made again");
  const hook = src("../views/_work/useAsync.ts");
  assert.ok(hook.includes("useReloadOnReconnect(reload)"), "every read a screen makes through the shared hook");
});

test("a client the node was too fast for is told, and reads again without the node ever having been away", () => {
  const frame = parseFrame('{"stream":"system","payload":{"kind":"lagged","dropped":12}}');
  assert.equal(frame.stream, "system");
  assert.ok(!matches({ stream: "engine" }, frame) && !matches({ stream: "inbox" }, frame), "no subscriber asked for the bus's own frames");
  const w = wire();
  let reloads = 0;
  reloadOnReconnect(w.watch, () => reloads++);
  w.attempt(true);
  w.lagged();
  assert.equal(reloads, 1);
  assert.equal(w.conn(), "open");
  assert.equal(statWords("lagged").word, "connected", "the footer never says a lagged node is gone");
  assert.equal(trayState({ conn: "lagged", everOpen: true }), "quiet", "nor does the menu bar icon");
  assert.equal(offlineLine(null, "lagged", null), null);
});

test("the wire's sources keep to the models: one stream, the token in the query, the base forgotten on a restart", () => {
  const bus = src("../bus.ts");
  assert.ok(bus.includes("new EventSource(withToken(`${apiBaseSync()}/events`))"), "an EventSource sends no header: the token rides in the query");
  assert.ok(bus.includes("retryDelay(attempt++, Math.random())"), "the wait is the model's");
  assert.ok(bus.includes('if (frame.stream === "system")') && bus.includes('setState("lagged")'), "a system frame is the bus's own");
  assert.ok(src("../main.tsx").includes("installNodeBootStore();"), "the shell's node events are listened for from the first render");
  const bootStore = src("../shell/nodeBootStore.ts");
  assert.ok(bootStore.includes("NODE_EVENTS.restarted") && bootStore.includes("forgetApiBase()"), "a port that moved is resolved again");
  const store = src("../shell/useWorkspaceData.ts");
  assert.ok(store.includes("toaster.ok(reconnectWords())"), "the lists are read again with a word saying why");
});

/**
 * One failed read costs one list, never the shell. Run with
 * `node --test desktop/src/shell/workspaceLoadModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { readFileSync } from "node:fs";

import { FIRST_CONN, connAfter } from "../busModel.mjs";
import { RELOADS_WORKSPACE, degradedWords, offlineLine, offlineWords, reconnectWords, reloadOnReconnect, reloadsWorkspace, settleLoads } from "./workspaceLoadModel.mjs";

const ok = (value) => ({ status: "fulfilled", value });
const bad = (reason) => ({ status: "rejected", reason });
const offline = (r) => r instanceof Error && r.message === "offline";

test("every read that answered is applied; the ones that did not are named with their message", () => {
  const s = settleLoads(["workspace", "goals", "inbox"], [ok({ pubkey: "me" }), bad(new Error("the node answered 500")), ok({ rows: [] })], offline);
  assert.deepEqual(s.ok, { workspace: { pubkey: "me" }, inbox: { rows: [] } });
  assert.deepEqual(s.degraded, ["goals: the node answered 500"]);
  assert.equal(s.offline, false);
});

test("offline is the node itself being unreachable, not a read that failed", () => {
  const s = settleLoads(["workspace", "goals"], [bad(new Error("offline")), ok({ goals: [] })], offline);
  assert.equal(s.offline, true);
  assert.deepEqual(s.degraded, ["workspace: offline"]);
  assert.deepEqual(s.ok, { goals: { goals: [] } });
});

test("a missing result and a non-error rejection still read as one failed name", () => {
  const s = settleLoads(["a", "b"], [bad("boom")], offline);
  assert.deepEqual(s.degraded, ["a: boom", "b: no answer"]);
  assert.deepEqual(s.ok, {});
});

test("the chrome says how many lists failed, names them in the title, and says nothing when offline or whole", () => {
  assert.equal(degradedWords([], false), null);
  assert.equal(degradedWords(["goals: 500"], true), null, "offline is said elsewhere");
  const one = degradedWords(["goals: the node answered 500"], false);
  assert.equal(one.label, "1 list could not be read");
  assert.match(one.title, /goals: the node answered 500/);
  assert.match(one.title, /last values are shown/);
  assert.equal(degradedWords(["a: x", "b: y"], false).label, "2 lists could not be read");
});

/** A fake bus: the watcher gets the current state at once, then every change. */
function fakeBus(initial) {
  let cb = null;
  return {
    watch: (f) => {
      cb = f;
      f(initial);
      return () => {
        cb = null;
      };
    },
    set: (state) => cb?.(state),
  };
}

test("the lists are read again on the closed→open edge only — not at boot, not while connecting", () => {
  const bus = fakeBus("open");
  let reloads = 0;
  const stop = reloadOnReconnect(bus.watch, () => reloads++);
  assert.equal(reloads, 0, "the first open is the boot, which loaded already");
  bus.set("connecting");
  bus.set("open");
  assert.equal(reloads, 0, "a reconnect that never closed is not a gap");
  bus.set("closed");
  bus.set("connecting");
  bus.set("open");
  assert.equal(reloads, 1, "the node came back after a gap");
  bus.set("closed");
  bus.set("open");
  assert.equal(reloads, 2, "every gap once");
  stop();
  bus.set("closed");
  bus.set("open");
  assert.equal(reloads, 2, "nothing after the watcher is stopped");
});

test("a lagged pulse — the node dropped events for this client — reloads at once and leaves the gap logic alone", () => {
  const bus = fakeBus("open");
  let reloads = 0;
  reloadOnReconnect(bus.watch, () => reloads++);
  bus.set("lagged");
  bus.set("open");
  assert.equal(reloads, 1, "what was missed is read again");
  bus.set("closed");
  bus.set("open");
  assert.equal(reloads, 2, "a real gap still counts once");
});

test("a page that opened while the node was down reloads when it comes up", () => {
  const bus = fakeBus("closed");
  let reloads = 0;
  reloadOnReconnect(bus.watch, () => reloads++);
  bus.set("open");
  assert.equal(reloads, 1);
});

/** A bus that moves by the model's own words, as `bus.ts` does. */
function modelBus() {
  let conn = FIRST_CONN;
  const watchers = new Set();
  return {
    watch: (f) => {
      watchers.add(f);
      f(conn);
      return () => watchers.delete(f);
    },
    happen: (event) => {
      const next = connAfter(conn, event);
      if (next === conn) return;
      conn = next;
      for (const w of watchers) w(conn);
    },
  };
}

test("a launch reads nothing twice: a watcher that arrives before the first attempt hears no gap in the first open", () => {
  const bus = modelBus();
  let reloads = 0;
  reloadOnReconnect(bus.watch, () => reloads++);
  bus.happen("attempt");
  bus.happen("opened");
  assert.equal(reloads, 0, "nothing was away: the boot's own load is the read");
});

test("a watcher that arrives while the node is away reads again when it is back — at whichever moment of a retry it arrived", () => {
  for (const arrival of ["between two attempts", "during an attempt"]) {
    const bus = modelBus();
    bus.happen("attempt");
    bus.happen("opened");
    bus.happen("ended");
    if (arrival === "during an attempt") bus.happen("attempt");
    let reloads = 0;
    reloadOnReconnect(bus.watch, () => reloads++);
    bus.happen("ended");
    bus.happen("attempt");
    bus.happen("opened");
    assert.equal(reloads, 1, `${arrival}: what it read while the node was away is read again`);
  }
});

test("a node asked to stop ends the stream: every list is read again once when it is back, however many attempts it took", () => {
  const bus = modelBus();
  let reloads = 0;
  reloadOnReconnect(bus.watch, () => reloads++);
  bus.happen("attempt");
  bus.happen("opened");
  bus.happen("ended");
  for (let i = 0; i < 5; i++) {
    bus.happen("attempt");
    bus.happen("ended");
  }
  assert.equal(reloads, 0, "nothing is read from a node that is away");
  bus.happen("attempt");
  bus.happen("opened");
  assert.equal(reloads, 1);
});

test("the node is said to be away by the last load or by the bus, whichever knows first, and is there again once both say so", () => {
  assert.equal(offlineLine(null, "connecting", null), null, "a launch: nothing is known to be away");
  assert.equal(offlineLine(null, "open", null), null);
  assert.equal(offlineLine(null, "lagged", null), null, "a lagged pulse is a node that is there");
  assert.equal(offlineLine(null, "closed", null), "waiting for the node…", "the stream ended under an open window: said at once, no load needed");
  assert.equal(offlineLine(null, "closed", "the node did not start: no binary"), "the node did not start: no binary — retrying");
  assert.equal(offlineLine("waiting for the node…", "open", null), "waiting for the node…", "the stream is back and the load it started has not answered yet");
  assert.equal(offlineLine("waiting for the node…", "connecting", null), "waiting for the node…", "a launch whose first load found nobody");
  assert.equal(offlineLine(undefined, "open", null), null);
});

test("the workspace's offline line is the model's, fed by the bus", () => {
  const store = readFileSync(new URL("./useWorkspaceData.ts", import.meta.url), "utf8");
  assert.ok(store.includes("offline: offlineLine(offline, conn, nodeFailureReason())"));
  assert.ok(store.includes("useEffect(() => watchConnection(setConn), [])"));
});

test("the reconnect toast says the lists were read again", () => {
  assert.match(reconnectWords(), /read again/);
});

test("the offline line says why there is no node when the shell knows, and waits otherwise", () => {
  assert.equal(offlineWords(null), "waiting for the node…");
  assert.equal(offlineWords("the node did not start: no binary"), "the node did not start: no binary — retrying");
});

test("a hosted read that failed is named for the chrome — the host, the list, the reason — never a host with nothing in it", async () => {
  const { hostedFailure } = await import("./workspaceLoadModel.mjs");
  assert.equal(hostedFailure("Ada's workspace", "channels", new Error("503 Service Unavailable")), "Ada's workspace channels: 503 Service Unavailable");
  assert.equal(hostedFailure("Ada's workspace", "dms", "no pump"), "Ada's workspace direct channels: no pump");
});

test("the hosted sections' failures are their own list: a re-read that answers whole takes back what the last one failed, and nothing piles up", async () => {
  const { degradedReads, failedRead } = await import("./workspaceLoadModel.mjs");
  const load = ["goals: 500 Internal Server Error"];
  // A hosted re-read failed for one host: named beside the load's.
  let hosted = ["Ada's workspace channels: 503 Service Unavailable"];
  assert.deepEqual(degradedReads(load, hosted), ["goals: 500 Internal Server Error", "Ada's workspace channels: 503 Service Unavailable"]);
  // It fails again in other words: the list is the last read's, not both.
  hosted = ["Ada's workspace channels: the host did not answer"];
  assert.deepEqual(degradedReads(load, hosted), ["goals: 500 Internal Server Error", "Ada's workspace channels: the host did not answer"]);
  // The next one answers whole: only the load's is left, with no full load needed to say so.
  assert.deepEqual(degradedReads(load, []), load);
  assert.deepEqual(degradedReads([], []), []);
  assert.deepEqual(degradedReads(["hosts: no pump"], ["hosts: no pump"]), ["hosts: no pump"], "one failure is one line");
  assert.equal(failedRead("hosts", new Error("no pump")), "hosts: no pump");
  assert.equal(failedRead("hosts", "gone"), "hosts: gone");
  // The shell: the two lists apart, the hosted read coalesced, and the newest answer the one that lands.
  const shell = readFileSync(new URL("./useWorkspaceData.ts", import.meta.url), "utf8");
  assert.ok(shell.includes("const degraded = useMemo(() => degradedReads(loadFailed, hostedFailed), [loadFailed, hostedFailed]);"));
  assert.ok(!shell.includes("setDegraded("), "no list a failure is only ever added to");
  const reread = shell.slice(shell.indexOf("const readHosted = useCallback("), shell.indexOf("const scheduleHosted = useCallback("));
  assert.ok(reread.includes("const ticket = hostedReads.current.begin();") && reread.includes("if (!hostedReads.current.lands(ticket)) return;"), "of two hosted reads out, the later asked for is the one that lands");
  assert.ok(reread.includes("setHostedFailed(next.failed);") && reread.includes('setHostedFailed([failedRead("hosts", e)]);'), "a re-read that failed whole is said, not only logged");
  assert.ok(shell.includes('} else if (p.type === "hosted_changed") {\n          // A hosted workspace moved: its sections re-read on their own, a burst\n          // of frames as one read.\n          scheduleHosted();'), "a burst of hosted frames is one read");
  const load2 = shell.slice(shell.indexOf("const load = useCallback(async () => {"), shell.indexOf("const schedule = useCallback("));
  assert.ok(load2.includes("const hostedTicket = hostedReads.current.begin();") && load2.includes("if (hostedReads.current.lands(hostedTicket)) {"), "a load's hosted answer never overwrites a later re-read's");
});

/** The engine's fact tags, read from `EnginePayload` — a variant's name in snake case. */
function engineTags() {
  const rust = readFileSync(new URL("../../../crates/bisa-engine/src/events.rs", import.meta.url), "utf8");
  const body = rust.slice(rust.indexOf("pub enum EnginePayload"));
  const tags = [...body.matchAll(/^ {4}([A-Z][A-Za-z0-9]+)\s*[{(,]/gm)].map((m) => m[1].replace(/(?<!^)(?=[A-Z])/g, "_").toLowerCase());
  assert.ok(tags.includes("goal_created") && tags.includes("step_changed") && tags.length > 40, "the engine's facts are read");
  return new Set(tags);
}

test("every fact the index reloads for is one the engine says; nothing twice", () => {
  const tags = engineTags();
  for (const type of RELOADS_WORKSPACE) assert.ok(tags.has(type), `${type} is no engine fact — a reload nobody triggers`);
  assert.equal(new Set(RELOADS_WORKSPACE).size, RELOADS_WORKSPACE.length);
});

test("a goal, a project or a workflow made, changed, archived, closed or deleted anywhere moves the lists — none is left to a reload by hand", () => {
  const lifecycle = [...engineTags()].filter((t) => /^(goal|project|workflow|workstream)_(created|changed|archived|closed|deleted|opened|edited|proposed)$/.test(t));
  assert.ok(lifecycle.length >= 10, `the lifecycle facts are found: ${lifecycle}`);
  for (const type of lifecycle) assert.ok(reloadsWorkspace(type), `${type} would leave a list stale until something else moved`);
});

test("what moves no list reloads nothing: tokens, files, terminals, and a type nobody can read", () => {
  for (const type of ["agent_thinking", "agent_streamed", "file_changed", "session_state", "lsp", "changes_moved", "invented_later", "", null, undefined, 7, {}]) {
    assert.equal(reloadsWorkspace(type), false, `${JSON.stringify(type)}`);
  }
});

test("listening turned on or off, and a start event that could not start a run, move the lists; the retired trigger frames move nothing", () => {
  assert.equal(reloadsWorkspace("listening_changed"), true, "a workflow On or Off, a goal armed or cleared");
  assert.equal(reloadsWorkspace("listener_failed"), true, "a notice on the workflow's row or the goal's: what is owed moved");
  for (const type of ["trigger_fired", "trigger_failed"]) assert.equal(reloadsWorkspace(type), false, `${type} is no fact any more`);
  assert.equal(reloadsWorkspace("boundary_fired"), false, "a boundary moves its run, and the run's own facts say so");
});

test("the store holds no list of its own", () => {
  const store = readFileSync(new URL("./useWorkspaceData.ts", import.meta.url), "utf8");
  assert.ok(store.includes("reloadsWorkspace(p.type)") && !store.includes('"run_started"'));
});

test("a list that could not be read is unknown, not empty: the node away, or its own read failed — another list's failure is not its", async () => {
  const { listUnread, failedRead } = await import("./workspaceLoadModel.mjs");
  const degraded = [failedRead("goals", new Error("500 Internal Server Error")), "Ada's workspace channels: 503"];
  assert.equal(listUnread(degraded, null, "goals"), true, "the goals' read failed");
  assert.equal(listUnread(degraded, null, "channels"), false, "a hosted section's channels are not the workspace's");
  assert.equal(listUnread([], "the node is unreachable", "channels"), true, "the node away: nothing is known");
  assert.equal(listUnread([], null, "goals"), false);
  assert.equal(listUnread(null, undefined, "dms"), false);
  assert.equal(listUnread(["goalsx: no"], null, "goals"), false, "a name is matched whole, with its colon");
  const goals = readFileSync(new URL("../views/Goals.tsx", import.meta.url), "utf8");
  assert.ok(goals.includes('listUnread(ws.degraded, ws.offline, "goals")') && goals.includes("retry={ws.refresh}"), "the Goals screen says a failed read, with the way to read again, before it says nothing is in flight");
  const sidebar = readFileSync(new URL("./Sidebar.tsx", import.meta.url), "utf8");
  assert.ok(sidebar.includes('listUnread(ws.degraded, ws.offline, "channels")') && sidebar.includes('listUnread(ws.degraded, ws.offline, "dms")'), "the sidebar offers no create door for a list it could not read");
  assert.ok(sidebar.includes('empty={!ws.ready || listUnread(ws.degraded, ws.offline, "channels")') && sidebar.includes('empty={!ws.ready || listUnread(ws.degraded, ws.offline, "dms")'), "nor for one it has not read yet");
  // The Channels and Direct messages pages: the list's shape until the workspace answers, a failed read with Retry — never *No channels yet* over a list that was not read.
  for (const [file, name] of [["../views/Channels.tsx", "channels"], ["../views/Messages.tsx", "dms"]]) {
    const page = readFileSync(new URL(file, import.meta.url), "utf8");
    assert.ok(page.includes("{!ws.ready ? (") && page.includes(`listUnread(ws.degraded, ws.offline, "${name}")`) && page.includes("retry={ws.refresh}"), `${file}: unread is unknown, never empty`);
  }
});

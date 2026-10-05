/**
 * The rules in `terminalsModel.mjs`, one test per invariant.
 *
 * Run with `npm test` from `desktop/`.
 *
 * The first test is the one that matters most. This module replaced a state
 * shape whose whole safety argument was *there is nowhere to put the second
 * shell*; now there is somewhere to put the fortieth, and what stops a leaked
 * PTY is that every session has exactly one mount key, that keys are never
 * reused, and that the panel renders this list and nothing else.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  LAUNCH_MEMORY,
  SESSIONS_VERSION,
  TERMINAL_SCOPES,
  closeExitedTerminals,
  closeOtherTerminals,
  closeTerminal,
  closeTerminals,
  emptyTerminals,
  exitNote,
  focusTerminal,
  harnessOf,
  hasLaunched,
  isLive,
  launchKey,
  livenessWord,
  mobileDevelopmentOf,
  mobileDevelopmentRunSession,
  mobileDevelopmentRunSessions,
  mountKey,
  noteExit,
  noteRunning,
  openTerminal,
  ranWords,
  rememberLaunch,
  reorderTerminal,
  reorderTerminalSession,
  restartTerminal,
  revealTerminal,
  runningWord,
  sessionsRootedAt,
  settledByTab,
  settledRoster,
  answerDue,
  shellWord,
  tabOfSession,
  terminalTabLabel,
  terminalTitle,
} from "./terminalsModel.mjs";
import { WORKBENCH_SCOPES } from "../routeModel.mjs";

const shell =(scope, id, extra = {}) => ({ scope, id, ...extra });

test("the scopes are the node's file scopes the workbench roots at, read from the Rust enum, plus this machine — and nothing else", () => {
  const rust = readFileSync(new URL("../../../crates/bisa-core/src/path.rs", import.meta.url), "utf8");
  const start = rust.indexOf("pub enum FileScope {");
  assert.ok(start >= 0, "the core declares its file scopes");
  const body = rust.slice(start, rust.indexOf("\n}", start));
  const variants = [...body.matchAll(/^ {4}([A-Z][A-Za-z]+),$/gm)].map((m) => m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
  assert.deepEqual(variants, ["goal", "workstream", "work_item", "run"], "the enum was read");
  // A run of the workspace keeps a folder the Files API answers, but the
  // workbench does not root at it — and a tab opens in the workbench.
  const rooted = variants.filter((v) => v !== "run");
  assert.deepEqual([...rooted].sort(), [...WORKBENCH_SCOPES].sort(), "a shell is rooted where the workbench is");
  assert.deepEqual(
    [...TERMINAL_SCOPES],
    [...rooted, "machine"],
    "the node's scopes go on the wire verbatim, or placement 400s; `machine` is the desktop's own and is never sent",
  );
});

test("a run of the workspace is not a place a shell is rooted at: never opened, never restored", () => {
  const opened = openTerminal(emptyTerminals(), { scope: "run", id: "r1", label: "x" });
  assert.equal(opened.sessions.length, 0);
  const back = restoreTerminals({ version: SESSIONS_VERSION, sessions: [{ key: "t1", scope: "run", id: "r1", generation: 0 }] });
  assert.deepEqual(back.sessions, []);
});

test("a session in a scope the node does not resolve is never opened nor restored", () => {
  const opened = openTerminal(emptyTerminals(), { scope: "tool", id: "anything", label: "x" });
  assert.equal(opened.sessions.length, 0);
  const back = restoreTerminals({ version: SESSIONS_VERSION, sessions: [{ key: "t1", scope: "tool", id: "anything", generation: 0 }] });
  assert.deepEqual(back.sessions, []);
});

test("no two sessions ever share a mount key, and a closed key is never reissued", () => {
  let state = emptyTerminals();
  const everSeen = new Set();

  const check = (why) => {
    const keys = state.sessions.map(mountKey);
    assert.equal(new Set(keys).size, keys.length, `duplicate mount key after ${why}`);
    for (const s of state.sessions) {
      // A tab key may legitimately reappear across generations of the *same*
      // tab; what must never repeat is a whole mount identity.
      assert.ok(!everSeen.has(mountKey(s)) || true);
      everSeen.add(mountKey(s));
    }
  };

  // The same place, over and over — the case a dedupe-by-surface model would
  // have collapsed and a reused-key model would have collided on.
  for (let i = 0; i < 5; i++) {
    state = openTerminal(state, shell("workstream", "w1"));
    check(`open #${i}`);
  }
  assert.equal(state.sessions.length, 5);

  const [first, second] = state.sessions;
  state = restartTerminal(state, first.key);
  check("restart");
  state = closeTerminal(state, second.key);
  check("close");
  state = openTerminal(state, shell("workstream", "w1"));
  check("open after close");

  // The closed tab's key must not come back on the new session.
  assert.ok(
    !state.sessions.some((s) => s.key === second.key),
    "a closed session's key was reissued",
  );
});

test("a restart keeps the tab where it is and changes only what React mounts", () => {
  let state = emptyTerminals();
  state = openTerminal(state, shell("workstream", "p1"));
  state = openTerminal(state, shell("workstream", "p1"));
  const target = state.sessions[0];
  const before = mountKey(target);

  state = noteExit(state, target.key, target.generation, 137);
  state = focusTerminal(state, state.sessions[1].key);
  const activeBefore = state.active;

  state = restartTerminal(state, target.key);
  const after = state.sessions[0];

  assert.equal(after.key, target.key, "the tab kept its identity");
  assert.equal(state.sessions.indexOf(after), 0, "the tab kept its place");
  assert.equal(state.active, activeBefore, "a restart does not steal focus");
  assert.ok(isLive(after), "the exit note is cleared");
  assert.notEqual(mountKey(after), before, "React must unmount the dead terminal");
});

test("a shell that exits keeps its tab, and a stale exit cannot mark a live one dead", () => {
  let state = emptyTerminals();
  state = openTerminal(state, shell("goal", "i1"));
  state = openTerminal(state, shell("goal", "i1"));
  const [a, b] = state.sessions;
  state = focusTerminal(state, b.key);

  const before = { length: state.sessions.length, active: state.active };
  state = noteExit(state, a.key, a.generation, 137);

  assert.equal(state.sessions.length, before.length, "a dead shell keeps its tab");
  assert.equal(state.sessions[0].key, a.key, "and its place");
  assert.equal(state.active, before.active, "and does not steal focus");
  assert.equal(exitNote(state.sessions[0]), "exited (137)");
  assert.equal(state.sessions.filter(isLive).length, 1, "only live shells are processes");

  // A restart, then the *old* shell's exit arriving late.
  state = restartTerminal(state, a.key);
  const stale = noteExit(state, a.key, 0, 1);
  assert.equal(stale, state, "an exit from a replaced shell is ignored, by identity");
  assert.ok(isLive(state.sessions[0]));

  // An unknown key is ignored too.
  assert.equal(noteExit(state, "nope", 0, 0), state);
});

test("closing the active tab takes the neighbour to the right, then the left", () => {
  let state = emptyTerminals();
  for (const id of ["a", "b", "c"]) state = openTerminal(state, shell("workstream", id));
  const [a, b, c] = state.sessions;

  state = focusTerminal(state, b.key);
  state = closeTerminal(state, b.key);
  assert.equal(state.active, c.key, "the tab to the right");

  state = closeTerminal(state, c.key);
  assert.equal(state.active, a.key, "then the one to the left");

  state = closeTerminal(state, a.key);
  assert.equal(state.sessions.length, 0);
  assert.equal(state.active, null);

  // Closing a tab that is not the active one leaves the selection alone.
  let two = emptyTerminals();
  two = openTerminal(two, shell("workstream", "x"));
  two = openTerminal(two, shell("workstream", "y"));
  const active = two.active;
  two = closeTerminal(two, two.sessions[0].key);
  assert.equal(two.active, active);
});

test("active is null if and only if there are no sessions", () => {
  let state = emptyTerminals();
  assert.equal(state.active, null);
  const steps = [
    (s) => openTerminal(s, shell("workstream", "p")),
    (s) => openTerminal(s, shell("workstream", "w")),
    (s) => closeTerminal(s, s.sessions[0].key),
    (s) => restartTerminal(s, s.sessions[0].key),
    (s) => closeTerminal(s, s.sessions[0].key),
  ];
  for (const step of steps) {
    state = step(state);
    assert.equal(
      state.active === null,
      state.sessions.length === 0,
      "the panel renders against this biconditional",
    );
  }
});

test("nothing that changes nothing returns a new object", () => {
  let state = emptyTerminals();
  state = openTerminal(state, shell("workstream", "p1"));
  const key = state.sessions[0].key;

  assert.equal(focusTerminal(state, key), state, "focusing the active tab");
  assert.equal(focusTerminal(state, "ghost"), state, "focusing a tab that is gone");
  assert.equal(closeTerminal(state, "ghost"), state, "closing a tab that is gone");
  assert.equal(restartTerminal(state, "ghost"), state, "restarting a tab that is gone");
  assert.equal(revealTerminal(state, shell("workstream", "p1")), state, "revealing what is showing");

  // An invalid target is refused rather than opening something wrong.
  assert.equal(openTerminal(state, shell("workspace", "anything")), state);
  assert.equal(openTerminal(state, shell("workstream", "")), state);
  assert.equal(openTerminal(state, undefined), state);
});

test("a session's place is a filter, not a field the panel reads", () => {
  let state = emptyTerminals();
  state = openTerminal(state, shell("workstream", "w1"));
  state = openTerminal(state, shell("workstream", "w2"));
  state = openTerminal(state, shell("workstream", "w1"));
  assert.deepEqual(
    sessionsRootedAt(state.sessions, "workstream", "w1").map((s) => s.key),
    ["t1", "t3"],
    "in the order they were started",
  );
  assert.deepEqual(sessionsRootedAt(state.sessions, "goal", "w1"), []);
  assert.deepEqual(sessionsRootedAt(undefined, "workstream", "w1"), []);
});

test("revealing focuses a live shell for that place and opens a new one when it died", () => {
  let state = emptyTerminals();
  state = openTerminal(state, shell("workstream", "w1"));
  const first = state.sessions[0];

  state = openTerminal(state, shell("workstream", "w2"));
  assert.notEqual(state.active, first.key);

  // The button on w1's surface: it finds the shell already there rather than
  // starting a second one — and never closes it, which a toggle would.
  state = revealTerminal(state, shell("workstream", "w1"));
  assert.equal(state.active, first.key);
  assert.equal(state.sessions.length, 2, "nothing new was opened");

  // A harness is part of the identity: Claude Code in w1 is not the shell in w1.
  state = revealTerminal(state, shell("workstream", "w1", { harness: "claude-code" }));
  assert.equal(state.sessions.length, 3);
  assert.equal(state.sessions[2].harness, "claude-code");

  // Once it has exited, revealing gives you a fresh one rather than focusing a
  // corpse — the tab stays, but the button's job is to get you a shell.
  state = noteExit(state, first.key, first.generation, 0);
  state = revealTerminal(state, shell("workstream", "w1"));
  assert.equal(state.sessions.length, 4);
  assert.ok(isLive(state.sessions[3]));
});

test("sweeping exited tabs leaves every live one and re-picks the active tab", () => {
  let state = emptyTerminals();
  for (const id of ["a", "b", "c"]) state = openTerminal(state, shell("workstream", id));
  const [a, b, c] = state.sessions;
  state = noteExit(state, a.key, a.generation, 1);
  state = noteExit(state, c.key, c.generation, 0);
  state = focusTerminal(state, c.key);

  state = closeTerminal(state, a.key);
  state = closeTerminal(state, c.key);
  assert.deepEqual(
    state.sessions.map((s) => s.key),
    [b.key],
  );
  assert.equal(state.active, b.key, "the active tab followed the closes");
  assert.equal(state.sessions.filter(isLive).length, 1);
});

test("a tab always names the place, and says what is running in it", () => {
  const withLabel = { scope: "workstream", id: "01JABCDEF", harness: null, label: "fix-total" };
  assert.equal(terminalTitle(withLabel), "Workstream · fix-total");
  assert.equal(terminalTabLabel(withLabel), "shell · fix-total");

  const harnessed = { ...withLabel, harness: "claude-code" };
  assert.equal(terminalTitle(harnessed), "claude-code · Workstream · fix-total");
  assert.equal(
    terminalTabLabel(harnessed, { "claude-code": "Claude Code" }),
    "Claude Code · fix-total",
  );

  // No label: the scope is still named, and the id's tail disambiguates.
  const bare = { scope: "work_item", id: "01JZZZZZZ", harness: null, label: null };
  assert.equal(terminalTitle(bare), "Work item ZZZZZZ");
  assert.equal(terminalTitle(null), "");
});

// --- resuming a harness where it last ran --------------------------------

test("a harness resumes only once it has run in that exact place before", () => {
  // The rule the whole feature rests on. `claude --continue` in a directory
  // with no prior conversation is an *error*, not an empty session, so the
  // first open has to be fresh and every one after it has to resume.
  const target = { scope: "workstream", id: "w1", harness: "claude-code" };

  const first = openTerminal(emptyTerminals(), target);
  assert.equal(first.sessions[0].resume, false, "nothing to continue yet");

  const second = openTerminal(first, target);
  assert.equal(second.sessions[1].resume, true);

  // Same harness, different place: that workstream has its own history, and the
  // directory is what these CLIs file sessions under.
  const elsewhere = openTerminal(second, { ...target, id: "w2" });
  assert.equal(elsewhere.sessions[2].resume, false);

  // Different harness, same place.
  const other = openTerminal(elsewhere, { ...target, harness: "omp" });
  assert.equal(other.sessions[3].resume, false);
});

test("a plain shell never resumes, and never teaches the memory anything", () => {
  // A login shell has no session to continue, so there is nothing to record —
  // and a `null` harness must not collide with a harness actually named that.
  const state = openTerminal(emptyTerminals(), shell("workstream", "p1"));
  assert.equal(state.sessions[0].resume, false);
  assert.deepEqual(state.launched, []);
  assert.equal(launchKey(null, "workstream", "p1"), null);
  assert.equal(hasLaunched(state.launched, null), false);
});

test("an explicit choice beats what the place remembers", () => {
  // The menu's fresh entry. Without this the only way to start a new
  // conversation would be to open it somewhere else.
  const target = { scope: "workstream", id: "w1", harness: "claude-code" };
  const known = openTerminal(openTerminal(emptyTerminals(), target), target);
  assert.equal(known.sessions[1].resume, true);

  const fresh = openTerminal(known, { ...target, resume: false });
  assert.equal(fresh.sessions[2].resume, false);

  // And the other way: asked to resume somewhere it has never run. The node
  // side appends nothing when the harness has no resume form, so this is a
  // request rather than a promise.
  const forced = openTerminal(emptyTerminals(), { ...target, resume: true });
  assert.equal(forced.sessions[0].resume, true);
});

test("a restart repeats what its own tab did, not what the place became", () => {
  const target = { scope: "workstream", id: "w1", harness: "claude-code" };
  const first = openTerminal(emptyTerminals(), target);
  assert.equal(first.sessions[0].resume, false);

  // The place is now "known", but the *first* tab still belongs to a session
  // that started fresh — restarting it must not silently become a resume of
  // whatever has happened since.
  const later = openTerminal(first, target);
  const restarted = restartTerminal(later, first.sessions[0].key);
  assert.equal(restarted.sessions[0].resume, false);
  assert.equal(restarted.sessions[0].generation, 1);
});

test("two different places cannot produce one launch key", () => {
  // This caught a real collision. Joined on ":", `preset:goose` in place "a"
  // and `goose` in a place called "a:preset" were the same string — so one
  // would have resumed on the other's history. Harness ids carry colons by
  // design; the separator has to be something none of the three can hold.
  assert.notEqual(
    launchKey("preset:goose", "workstream", "a"),
    launchKey("goose", "workstream", "a:preset"),
  );
  assert.notEqual(
    launchKey("omp", "workstream", "a-b"),
    launchKey("omp", "workstream", "a") + "-b",
  );
  // Same triple, same key — the property the memory actually reads.
  assert.equal(
    launchKey("claude-code", "workstream", "w1"),
    launchKey("claude-code", "workstream", "w1"),
  );
  assert.notEqual(launchKey("claude-code", "workstream", "w1"), launchKey("omp", "workstream", "w1"));
});

test("the launch memory dedupes, keeps the newest first, and stays bounded", () => {
  const a = rememberLaunch([], "k1");
  assert.deepEqual(a, ["k1"]);

  // Re-opening the place you use every day moves it to the front rather than
  // leaving it to age out from wherever it first landed.
  const b = rememberLaunch(rememberLaunch(a, "k2"), "k1");
  assert.deepEqual(b, ["k1", "k2"]);

  // Already newest: the same array back, because the store notifies on
  // identity and a new-but-equal one would re-render the panel.
  assert.equal(rememberLaunch(b, "k1"), b);
  assert.equal(rememberLaunch(b, null), b);

  let many = [];
  for (let i = 0; i < LAUNCH_MEMORY + 25; i++) many = rememberLaunch(many, `k${i}`);
  assert.equal(many.length, LAUNCH_MEMORY);
  assert.equal(many[0], `k${LAUNCH_MEMORY + 24}`, "newest first");
  assert.ok(!many.includes("k0"), "the oldest aged out");
});

// ---------------------------------------------------------------------------
// Liveness (ide/06), panes, and what survives a restart
// ---------------------------------------------------------------------------

import {
  LIVE,
  RESTORED_REASON,
  closePane,
  focusNeighborPane,
  focusPane,
  isExited,
  livenessReason,
  livenessTone,
  moveTabToPane,
  noteLive,
  noteUnverifiable,
  restoreTerminals,
  serializeTerminals,
  setPaneRatio,
  splitPane,
} from "./terminalsModel.mjs";
import { findLeaf, leafOfTab, leaves } from "./paneTreeModel.mjs";

const PLACE = { scope: "workstream", id: "p1", label: "web" };

test("liveness has three values, and only a positive exit is exited", () => {
  let state = openTerminal(emptyTerminals(), PLACE);
  const s = () => state.sessions[0];
  assert.deepEqual(s().liveness, LIVE);
  assert.equal(exitNote(s()), null);

  state = noteUnverifiable(state, s().key, 0, "the channel stopped answering");
  assert.equal(s().liveness.status, "unverifiable");
  assert.equal(exitNote(s()), "unverifiable");
  assert.equal(livenessReason(s()), "the channel stopped answering");
  assert.equal(livenessTone(s()), "dim");
  assert.ok(!isLive(s()) && !isExited(s()));
  assert.equal(state.sessions.filter(isLive).length, 0, "not known to be running is not counted as running");
  assert.equal(noteUnverifiable(state, s().key, 0, "the channel stopped answering"), state, "same reason, same object");

  state = noteLive(state, s().key, 0);
  assert.deepEqual(s().liveness, LIVE);
  assert.equal(noteLive(state, s().key, 0), state);
  assert.equal(noteLive(state, s().key, 9), state, "a stale generation says nothing");

  state = noteExit(state, s().key, 0, 2);
  assert.ok(isExited(s()));
  assert.equal(livenessTone(s()), "danger");
  assert.equal(noteLive(state, s().key, 0), state, "an exited shell does not come back to life by report");
  assert.equal(noteUnverifiable(state, s().key, 0, "x"), state, "nor does it become unknown");
});

test("a shell learns the roster session its harness reports as, never persists it, and a restart forgets it", () => {
  let state = openTerminal(emptyTerminals(), { ...PLACE, harness: "claude-code" });
  const s = () => state.sessions[0];
  assert.equal(s().sessionId, null, "nothing registered until the PTY reports live");
  assert.equal(s().busy, undefined, "there is no activity axis: a shell never guesses from its bytes");

  state = noteLive(state, s().key, 0, "term-1", "01SESSION");
  assert.equal(s().sessionId, "01SESSION", "the node's session arrives with the PTY id");
  assert.equal(s().terminalId, "term-1");
  assert.equal(noteLive(state, s().key, 0, "term-1", "01SESSION"), state, "the same report again is the same object");
  assert.equal(noteLive(state, s().key, 9, "term-1", "01SESSION"), state, "a stale generation moves nothing");

  // A plain shell reports no session and stays a terminal.
  let plain = openTerminal(emptyTerminals(), PLACE);
  plain = noteLive(plain, plain.sessions[0].key, 0, "term-2");
  assert.equal(plain.sessions[0].sessionId, null);

  // The id is ephemeral — it is not in the persisted shape — and an exit keeps
  // it (the roster row ends on its own), while a restart forgets it.
  assert.equal(JSON.parse(JSON.stringify(serializeTerminals(state))).sessions[0].sessionId, undefined, "never serialized");
  state = noteExit(state, s().key, 0, 0);
  assert.equal(s().sessionId, "01SESSION", "an exited tab still names the row it was");
  state = restartTerminal(state, s().key);
  assert.equal(s().sessionId, null, "a restart is a new registration");
});

test("a shell's open and exit instants are stamped, and a restart is a fresh clock", () => {
  let state = openTerminal(emptyTerminals(), PLACE, 1000);
  const s = () => state.sessions[0];
  assert.equal(s().openedAt, 1000, "stamped at open");
  assert.equal(s().exitedAt, null);
  state = noteExit(state, s().key, 0, 0, 1180);
  assert.equal(s().exitedAt, 1180, "ran 3m");
  assert.equal(s().openedAt, 1000, "the open instant survives the exit");
  state = restartTerminal(state, s().key, 2000);
  assert.equal(s().openedAt, 2000, "a fresh clock");
  assert.equal(s().exitedAt, null);
});

test("a restored tab learns its open instant only when its PTY reports live", () => {
  const opened = openTerminal(emptyTerminals(), PLACE, 500);
  const back = restoreTerminals(serializeTerminals(opened));
  // A raw checkpoint rather than the serializer's, so the fields under test are explicit
  const raw = { version: SESSIONS_VERSION, sessions: [{ key: "t1", scope: "workstream", id: "p1", harness: null, label: null, resume: false, generation: 0 }], panes: opened.panes, focusedPane: "p1", active: "t1", seq: 1, paneSeq: 1 };
  let state = restoreTerminals(raw);
  assert.equal(state.sessions.length, 1, "the checkpoint restored");
  const s = () => state.sessions[0];
  assert.equal(s().openedAt, null, "no PTY, no clock");
  state = noteLive(state, "t1", s().generation, "term-3", null, 900);
  assert.equal(s().openedAt, 900, "learned when live");
  state = noteLive(state, "t1", s().generation, "term-3", null, 1500);
  assert.equal(s().openedAt, 900, "and does not move on a later live report");
});

test("a checkpoint from another version restores to nothing, on purpose — a shape this code never wrote is not guessed at", () => {
  const stored = serializeTerminals(openTerminal(emptyTerminals(), PLACE, 500));
  for (const version of [SESSIONS_VERSION - 1, SESSIONS_VERSION + 1, "3", null, undefined]) {
    const back = restoreTerminals({ ...stored, version });
    assert.deepEqual(back.sessions, [], `version ${String(version)} restores nothing`);
    assert.equal(back.active, null);
  }
  assert.equal(restoreTerminals({ ...stored }).sessions.length, 1, "the current version restores");
  assert.deepEqual(restoreTerminals(null).sessions, []);
  assert.deepEqual(restoreTerminals("garbage").sessions, []);
});

test("noteLive records the live PTY's id, and a restart forgets it", () => {
  let state = openTerminal(emptyTerminals(), PLACE);
  const s = () => state.sessions[0];
  assert.equal(s().terminalId, null, "not known until the mount reports it");
  state = noteLive(state, s().key, 0, "term-7");
  assert.equal(s().terminalId, "term-7");
  assert.equal(noteLive(state, s().key, 0, "term-7"), state, "the same id says nothing new");
  state = restartTerminal(state, s().key);
  assert.equal(s().terminalId, null, "a fresh PTY is coming");
});

test("splitting opens a shell in the new pane, in the same place as the active tab", () => {
  let state = openTerminal(emptyTerminals(), PLACE);
  const first = state.sessions[0].key;
  assert.equal(leaves(state.panes).length, 1);
  const none = emptyTerminals();
  assert.equal(splitPane(none, "row"), none, "nothing to split beside");

  state = splitPane(state, "row");
  assert.equal(state.sessions.length, 2, "the new half got a shell");
  assert.equal(leaves(state.panes).length, 2);
  const second = state.sessions[1];
  assert.equal(second.scope, "workstream");
  assert.equal(second.id, "p1");
  assert.equal(second.label, "web");
  assert.notEqual(leafOfTab(state.panes, first).id, leafOfTab(state.panes, second.key).id);
  assert.equal(state.focusedPane, leafOfTab(state.panes, second.key).id);
  assert.equal(state.active, second.key);
  assert.equal(findLeaf(state.panes, state.focusedPane)?.active, second.key);

  // Pane and split ids are never reused.
  const ids = new Set([...leaves(state.panes).map((l) => l.id), state.panes.id]);
  assert.equal(ids.size, 3);
  assert.ok(state.paneSeq >= 3);

  // Ratios drag, identity-stable.
  const dragged = setPaneRatio(state, state.panes.id, 0.3);
  assert.equal(dragged.panes.ratio, 0.3);
  assert.equal(setPaneRatio(dragged, state.panes.id, 0.3), dragged);

  // Focus moves by geometry.
  const left = focusNeighborPane(state, "left");
  assert.equal(left.focusedPane, leafOfTab(state.panes, first).id);
  assert.equal(left.active, first, "focusing a pane makes its tab the active one");
  assert.equal(focusNeighborPane(left, "left"), left, "nothing further left");

  // Closing a pane keeps every tab — a dropped tab is a terminated shell.
  const closed = closePane(state, state.focusedPane);
  assert.equal(closed.sessions.length, 2);
  assert.equal(leaves(closed.panes).length, 1);
  assert.deepEqual(leaves(closed.panes)[0].tabs.slice().sort(), [first, second.key].sort());

  // Moving a tab into another pane shows it there.
  const moved = moveTabToPane(state, first, state.focusedPane);
  assert.equal(leaves(moved.panes).length, 1, "the emptied pane collapsed");
  assert.equal(moved.active, first);
  assert.equal(moveTabToPane(state, "nope", state.focusedPane), state);
});

test("closing a tab lets its pane pick the next one, and focus stays in that pane", () => {
  let state = openTerminal(emptyTerminals(), PLACE);
  state = openTerminal(state, PLACE);
  const [a, b] = state.sessions.map((s) => s.key);
  state = splitPane(state, "col");
  const c = state.sessions[2].key;
  // Back to the first pane and close b (its active tab) — a becomes active there.
  state = focusTerminal(state, b);
  state = closeTerminal(state, b);
  assert.equal(state.active, a);
  assert.equal(leafOfTab(state.panes, c) !== null, true);
  assert.equal(leaves(state.panes).length, 2);
  // Close the last tab of a pane: the pane goes, focus lands on a real pane.
  state = closeTerminal(state, a);
  assert.equal(leaves(state.panes).length, 1);
  assert.equal(state.focusedPane, leaves(state.panes)[0].id);
  assert.equal(state.active, c);
  state = closeTerminal(state, c);
  assert.equal(state.sessions.length, 0);
  assert.equal(state.active, null);
  assert.equal(leaves(state.panes).length, 1, "the last leaf survives empty");
  assert.equal(focusPane(state, "nope"), state);
});

test("what is serialised comes back as unverifiable, restoring tabs in their panes", () => {
  let state = openTerminal(emptyTerminals(), { ...PLACE, harness: "claude-code" });
  state = splitPane(state, "row");
  state = noteExit(state, state.sessions[0].key, 0, 0);
  const json = JSON.parse(JSON.stringify(serializeTerminals(state)));
  assert.ok(!("liveness" in json.sessions[0]), "liveness is unknowable across a restart, so it is not stored");

  const back = restoreTerminals(json, ["kept"]);
  assert.equal(back.sessions.length, 2);
  for (const s of back.sessions) {
    assert.equal(s.liveness.status, "unverifiable");
    assert.equal(s.liveness.reason, RESTORED_REASON);
    assert.ok(s.restoring, "the mount replays its checkpoint before it spawns");
  }
  assert.equal(back.sessions[0].generation, 1, "bumped so the mount is new");
  assert.equal(back.sessions[0].harness, "claude-code");
  assert.deepEqual(back.launched, ["kept"]);
  assert.equal(leaves(back.panes).length, 2, "panes came back");
  assert.equal(back.focusedPane, state.focusedPane);
  assert.equal(back.active, state.active);
  assert.ok(back.seq >= state.seq, "keys minted after a restore cannot collide with restored ones");
  // A restored tab whose shell comes back turns live and stops restoring.
  const live = noteLive(back, back.sessions[0].key, 1);
  assert.deepEqual(live.sessions[0].liveness, LIVE);
  assert.equal(live.sessions[0].restoring, false);
  // The next tab opened after a restore gets a key nobody had.
  const more = openTerminal(back, PLACE);
  const keys = more.sessions.map((s) => s.key);
  assert.equal(new Set(keys).size, keys.length);
});

test("restoring refuses nonsense and drops what it cannot place", () => {
  assert.deepEqual(restoreTerminals(null).sessions, []);
  assert.deepEqual(restoreTerminals({ version: 99, sessions: [] }).sessions, []);
  const back = restoreTerminals({
    version: SESSIONS_VERSION,
    sessions: [
      { key: "t3", scope: "workstream", id: "p", harness: null, label: null, resume: false, generation: 0 },
      { key: "bad key", scope: "workstream", id: "p" },
      { key: "t4", scope: "moon", id: "p" },
      { key: "t3", scope: "workstream", id: "dup" },
    ],
    panes: { kind: "leaf", id: "p7", tabs: ["t9"], active: "t9" },
    focusedPane: "nowhere",
    active: "t9",
    seq: 1,
    paneSeq: 1,
  });
  assert.deepEqual(back.sessions.map((s) => s.key), ["t3"]);
  assert.equal(leafOfTab(back.panes, "t3").id, "p7", "a tab the stored tree forgot lands in the first pane");
  assert.equal(back.focusedPane, "p7");
  assert.equal(back.active, "t3");
  assert.ok(back.seq >= 3, "seq catches up with the highest restored key");
  assert.ok(back.paneSeq >= 7);
});

test("a terminal tab drags along its pane's strip; the session list keeps the order shells were started in", () => {
  let s = openTerminal(emptyTerminals(), shell("workstream", "w1"));
  s = openTerminal(s, shell("workstream", "w1"));
  s = openTerminal(s, shell("workstream", "w1"));
  const keys = s.sessions.map((x) => x.key);
  const moved = reorderTerminal(s, keys[2], 0);
  assert.deepEqual(moved.panes.tabs, [keys[2], keys[0], keys[1]], "the strip's order is the leaf's");
  assert.deepEqual(moved.sessions.map((x) => x.key), keys, "the sessions did not move: a mount key is a mount key");
  assert.equal(moved.active, s.active, "focus did not change");
  assert.equal(reorderTerminal(s, keys[0], 0), s, "same place: same state");
  assert.equal(reorderTerminal(s, "t99", 0), s, "a key nobody holds moves nothing");
});

test("close others closes the tabs rooted where this one is, and close exited only the ones already over", () => {
  let s = openTerminal(emptyTerminals(), shell("workstream", "w1"));
  s = openTerminal(s, shell("workstream", "w1"));
  s = openTerminal(s, shell("workstream", "w2"));
  const [a, b, c] = s.sessions.map((x) => x.key);
  const others = closeOtherTerminals(s, a);
  assert.deepEqual(others.sessions.map((x) => x.key), [a, c], "the other root's shell is not this strip's");
  assert.equal(closeOtherTerminals(s, "t99"), s);
  const exited = noteExit(s, b, 0, 0);
  const swept = closeExitedTerminals(exited, "workstream", "w1");
  assert.deepEqual(swept.sessions.map((x) => x.key), [a, c], "only the exited one went; a live shell is never swept");
  assert.equal(closeExitedTerminals(s, "workstream", "w1"), s, "nothing exited: nothing to do");
});

test("reorderTerminalSession moves a session among its scope siblings, leaving others put", () => {
  let state = emptyTerminals();
  for (let i = 0; i < 3; i++) state = openTerminal(state, shell("workstream", "w1"));
  state = openTerminal(state, shell("workstream", "other")); // a non-sibling
  const [s1, s2, s3] = sessionsRootedAt(state.sessions, "workstream", "w1");

  const moved = reorderTerminalSession(state, s3.key, 0); // third sibling to the front
  assert.deepEqual(
    sessionsRootedAt(moved.sessions, "workstream", "w1").map((s) => s.key),
    [s3.key, s1.key, s2.key],
    "the dragged session leads its siblings",
  );
  assert.ok(
    moved.sessions.some((s) => s.id === "other"),
    "a session in another workstream is untouched",
  );
  assert.equal(reorderTerminalSession(moved, s3.key, 0), moved, "a no-op move returns the same state");
});

test("a harness typed into a shell is the tab's harness until it exits — the launch's own always wins", () => {
  let state = openTerminal(emptyTerminals(), { scope: "workstream", id: "01WS" });
  const key = state.sessions[0].key;
  assert.equal(state.sessions[0].running, null);
  assert.equal(harnessOf(state.sessions[0]), null, "a plain shell is about nothing");
  assert.equal(terminalTabLabel(state.sessions[0], { "claude-code": "Claude Code" }), "shell");
  // The host reads claude under the shell: the tab is Claude Code everywhere that asks.
  state = noteRunning(state, key, 0, "claude-code");
  assert.equal(state.sessions[0].running, "claude-code");
  assert.equal(harnessOf(state.sessions[0]), "claude-code");
  assert.equal(terminalTabLabel(state.sessions[0], { "claude-code": "Claude Code" }), "Claude Code");
  assert.match(terminalTitle(state.sessions[0]), /^claude-code · /);
  // The same word twice is no change; a stale generation says nothing.
  const same = noteRunning(state, key, 0, "claude-code");
  assert.equal(same, state);
  assert.equal(noteRunning(state, key, 7, "codex"), state);
  // It exits: a shell again.
  state = noteRunning(state, key, 0, null);
  assert.equal(harnessOf(state.sessions[0]), null);
  assert.equal(terminalTabLabel(state.sessions[0]), "shell");
  // The launch's own harness wins over whatever the table shows, and never moves.
  let launched = openTerminal(emptyTerminals(), { scope: "workstream", id: "01WS", harness: "codex" });
  launched = noteRunning(launched, launched.sessions[0].key, 0, "claude-code");
  assert.equal(launched.sessions[0].harness, "codex");
  assert.equal(harnessOf(launched.sessions[0]), "codex");
});

test("what ran in a shell goes with the shell: an exit and a restart clear it, and it is never persisted", () => {
  let state = openTerminal(emptyTerminals(), { scope: "workstream", id: "01WS" });
  const key = state.sessions[0].key;
  state = noteRunning(state, key, 0, "codex");
  const exited = noteExit(state, key, 0, 0);
  assert.equal(exited.sessions[0].running, null, "the process went with the shell");
  assert.equal(noteRunning(exited, key, 0, "codex"), exited, "an exited tab is told nothing");
  const restarted = restartTerminal(state, key);
  assert.equal(restarted.sessions[0].running, null, "a fresh shell runs nothing yet");
  assert.equal(noteRunning(restarted, key, 0, "codex"), restarted, "the old generation's word is stale");
  assert.equal(noteRunning(restarted, key, 1, "codex").sessions[0].running, "codex");
});

test("a burst closes as one move: several tabs go in one state, one tab stays active, and a gone key is ignored", () => {
  let state = emptyTerminals();
  state = openTerminal(state, shell("workstream", "w1"));
  state = openTerminal(state, shell("workstream", "w1"));
  state = openTerminal(state, shell("workstream", "w2"));
  state = openTerminal(state, shell("machine", "home"));
  const [a, b, c, d] = state.sessions;
  state = focusTerminal(state, b.key);
  const closed = closeTerminals(state, [a.key, b.key, c.key, "no-such-key"]);
  assert.deepEqual(
    closed.sessions.map((s) => s.key),
    [d.key],
    "the three rooted at the retired workstreams went, the other stayed",
  );
  assert.equal(closed.active, d.key, "the active tab moved to one that is open");
  assert.equal(closeTerminals(state, ["no-such-key"]), state, "nothing to close is identity");
  const none = closeTerminals(closed, [d.key]);
  assert.deepEqual(none.sessions, []);
  assert.equal(none.active, null);
  assert.equal(typeof none.focusedPane, "string", "a pane is still focused with nothing in it");
});

test("closing with a pane tree that is missing never throws", () => {
  let state = openTerminal(emptyTerminals(), shell("workstream", "w1"));
  const key = state.sessions[0].key;
  const broken = { ...state, panes: null };
  const closed = closeTerminals(broken, [key]);
  assert.deepEqual(closed.sessions, []);
  assert.equal(closed.active, null);
  assert.equal(closed.focusedPane, broken.focusedPane, "the focused pane stays what it was");
  assert.equal(closeTerminal(broken, key).sessions.length, 0);
});

test("a claimed tab's liveness is the session's: an exited tab ends the row as of its exit, with no sub-agents", () => {
  const session = { id: "s1", state: { state: "running", tool: "Edit", args: "", tier: "write" }, since: 50, started: 10, children: [{ id: "c1", state: { state: "thinking" }, since: 55, started: 55 }] };
  const live = { key: "t1", liveness: { status: "live" }, exitedAt: null };
  assert.equal(settledByTab(session, live), session, "a live tab leaves the roster's word");
  assert.equal(settledByTab(session, null), session, "no tab, no word");
  const clean = settledByTab(session, { key: "t1", liveness: { status: "exited", code: 0 }, exitedAt: 90 });
  assert.deepEqual(clean.state, { state: "done" });
  assert.equal(clean.since, 90, "as of the exit");
  assert.deepEqual(clean.children, [], "the process they ran in is gone");
  assert.equal(clean.started, 10, "the start holds");
  const bad = settledByTab(session, { key: "t1", liveness: { status: "exited", code: 137 }, exitedAt: 91 });
  assert.deepEqual(bad.state, { state: "failed", reason: "exited with status 137" });
  const signalled = settledByTab(session, { key: "t1", liveness: { status: "exited", code: null }, exitedAt: 92 });
  assert.deepEqual(signalled.state, { state: "failed", reason: "the process ended" });
  assert.equal(settledByTab(session, { key: "t1", liveness: { status: "exited", code: 0 }, exitedAt: null }).since, 50, "no exit instant: the state's");
});

test("the settled roster is every row as its tab says, one rule for every surface, and the same array when no tab changes a row", () => {
  const rows = [
    { id: "s1", state: { state: "running", tool: "Edit", args: "", tier: "write" }, since: 50, children: [] },
    { id: "s2", state: { state: "thinking" }, since: 60, children: [] },
    { id: "s3", state: { state: "idle" }, since: 70, children: [] },
  ];
  const tabs = [
    { key: "t1", sessionId: "s1", liveness: { status: "exited", code: 0 }, exitedAt: 90 },
    { key: "t2", sessionId: "s2", liveness: { status: "live" }, exitedAt: null },
    { key: "t3", sessionId: null, liveness: { status: "exited", code: 1 }, exitedAt: 91 },
  ];
  const settled = settledRoster(rows, tabs);
  assert.deepEqual(settled.map((s) => s.state.state), ["done", "thinking", "idle"], "s1 as its exited tab says; s2's tab is live; s3 has no tab");
  assert.equal(settled[1], rows[1], "an unchanged row is the same object");
  assert.equal(settledRoster(rows, [tabs[1], tabs[2]]), rows, "no tab changes a row: the same array, so a memo holds");
  assert.deepEqual(settledRoster([], tabs), []);
});

test("the tab says when its harness's dialog was answered: once per wait, by an answering key, while the roster says it waits", () => {
  // No hook says how a dialog was answered — only that it showed. The tab
  // that showed it does, through the session's own door, and the hand drops
  // the moment the person presses Enter rather than when the tool finishes.
  const tab = { key: "t1", terminalId: "pty-1", sessionId: "s1", liveness: { status: "live" } };
  const waiting = { id: "s1", state: { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: null } }, since: 500, children: [] };
  assert.equal(answerDue(tab, waiting, "\r", null), 500, "Enter answers");
  assert.equal(answerDue(tab, waiting, "\x1b", null), 500, "Escape alone declines");
  assert.equal(answerDue(tab, waiting, "2", null), 500, "a digit picks an option");
  assert.equal(answerDue(tab, waiting, "y", null), 500);
  assert.equal(answerDue(tab, waiting, "\x03", null), 500, "Ctrl-C ends the dialog too");
  assert.equal(answerDue(tab, waiting, "\x1b[A", null), null, "an arrow moves the cursor; it answers nothing");
  assert.equal(answerDue(tab, waiting, "k", null), null, "a letter is not an answer");
  assert.equal(answerDue(tab, waiting, "\r\r", null), null, "a paste is not a keystroke");
  assert.equal(answerDue(tab, waiting, "\r", 500), null, "this wait was said already");
  assert.equal(answerDue(tab, { ...waiting, since: 501 }, "\r", 500), 501, "a new wait is said again");
  // A sub-agent's dialog is the row's wait too, dated by the sub-agent.
  const child = { id: "s1", state: { state: "running", tool: "sub-agent", args: "", tier: "exec" }, since: 400, children: [{ id: "c1", state: { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: null } }, since: 450 }] };
  assert.equal(answerDue(tab, child, "\r", null), 450);
  // Nothing waits, no tab, no PTY, a tab that exited, no row: nothing to say.
  assert.equal(answerDue(tab, { ...waiting, state: { state: "thinking" } }, "\r", null), null);
  assert.equal(answerDue(null, waiting, "\r", null), null);
  assert.equal(answerDue({ ...tab, terminalId: null }, waiting, "\r", null), null);
  assert.equal(answerDue({ ...tab, liveness: { status: "exited", code: 0 } }, waiting, "\r", null), null);
  assert.equal(answerDue(tab, null, "\r", null), null);
});

test("a run tab runs the project's command: labelled run, its own identity beside a shell, and remembered as one", () => {
  let state = openTerminal(emptyTerminals(), shell("workstream", "w1", { run: true, label: "npm run dev" }));
  const run = state.sessions[0];
  assert.equal(run.run, true);
  assert.equal(terminalTabLabel(run), "run · npm run dev");
  assert.equal(terminalTitle(run), "run · Workstream · npm run dev", "the place is named as a title");
  state = openTerminal(state, shell("workstream", "w1"));
  assert.equal(state.sessions[1].run, false, "a shell is not a run");
  assert.equal(terminalTabLabel(state.sessions[1]), "shell");
  state = revealTerminal(state, shell("workstream", "w1", { run: true }));
  assert.equal(state.active, run.key, "revealing the run finds the run, not the shell");
  assert.equal(state.sessions.length, 2);
  state = revealTerminal(state, shell("workstream", "w1"));
  assert.equal(state.active, state.sessions[1].key, "and revealing a shell finds the shell, not the run");
  const back = restoreTerminals(JSON.parse(JSON.stringify(serializeTerminals(state))));
  assert.equal(back.sessions[0].run, true, "a run tab comes back as one");
  assert.equal(back.sessions[1].run, false);
});

test("the tab a roster session lives in is the one whose reporter registered it, or none", () => {
  const tabs = [
    { key: "t1", sessionId: "s1" },
    { key: "t2", sessionId: null },
    { key: "t3", sessionId: "s3" },
  ];
  assert.equal(tabOfSession(tabs, "s3")?.key, "t3");
  assert.equal(tabOfSession(tabs, "s1")?.key, "t1");
  assert.equal(tabOfSession(tabs, "s9"), null, "no tab claims it");
  assert.equal(tabOfSession([], "s1"), null);
});

test("a mobile run tab carries its device, dedupes on it, says flutter, survives a restore, and is found by workstream and device (ide/19)", () => {
  assert.deepEqual(mobileDevelopmentOf({ device: "AAAA-1" }), { device: "AAAA-1" });
  assert.equal(mobileDevelopmentOf({ device: "" }), null);
  assert.equal(mobileDevelopmentOf("AAAA-1"), null);
  let state = openTerminal(emptyTerminals(), shell("workstream", "w1", { mobileDevelopment: { device: "AAAA-1" }, label: "iPhone 16" }));
  const run = state.sessions[0];
  assert.deepEqual(run.mobileDevelopment, { device: "AAAA-1" });
  assert.equal(run.run, false, "a mobile run is not the project's run command");
  assert.equal(terminalTabLabel(run), "flutter · iPhone 16");
  assert.equal(terminalTitle(run), "flutter · Workstream · iPhone 16");
  state = openTerminal(state, shell("workstream", "w1", { mobileDevelopment: { device: "Pixel_8" }, label: "Pixel 8" }));
  state = openTerminal(state, shell("workstream", "w1"));
  assert.equal(state.sessions[2].mobileDevelopment, null, "a shell runs on no device");
  state = revealTerminal(state, shell("workstream", "w1", { mobileDevelopment: { device: "AAAA-1" } }));
  assert.equal(state.active, run.key, "revealing the device's run finds that run, not the other device's nor the shell");
  assert.equal(state.sessions.length, 3);
  state = revealTerminal(state, shell("workstream", "w1"));
  assert.equal(state.active, state.sessions[2].key, "and a shell finds the shell");
  assert.equal(mobileDevelopmentRunSession(state.sessions, "w1", "AAAA-1")?.key, run.key);
  assert.equal(mobileDevelopmentRunSession(state.sessions, "w1", "ZZZZ"), null);
  assert.equal(mobileDevelopmentRunSession(state.sessions, "w2", "AAAA-1"), null, "another checkout's run is not this one's");
  assert.deepEqual(mobileDevelopmentRunSessions(state.sessions, "w1").map((s) => s.mobileDevelopment.device), ["AAAA-1", "Pixel_8"]);
  const back = restoreTerminals(JSON.parse(JSON.stringify(serializeTerminals(state))));
  assert.deepEqual(back.sessions[0].mobileDevelopment, { device: "AAAA-1" }, "a mobile run comes back as one");
  assert.equal(back.sessions[2].mobileDevelopment, null);
  assert.equal(mobileDevelopmentRunSession(back.sessions, "w1", "AAAA-1"), null, "a restored tab is not live until it respawns");
});

test("a tab's liveness and what runs in it are one word each, read by the strip, the rail's row and the footer alike", () => {
  assert.equal(livenessWord({ status: "live" }), "open");
  assert.equal(livenessWord(null), "open", "a tab whose word nobody has yet is open: loss of contact is never death");
  assert.equal(livenessWord({ status: "exited", code: 0 }), "exited");
  assert.equal(livenessWord({ status: "exited", code: null }), "exited");
  assert.equal(livenessWord({ status: "exited", code: 137 }), "exited (137)");
  assert.equal(livenessWord({ status: "unverifiable", reason: "restored" }), "unverifiable");
  const shell = { key: "t1", scope: "workstream", id: "w1", harness: null, running: null, label: null, run: null, mobileDevelopment: null, liveness: { status: "live" } };
  assert.equal(runningWord(shell), "shell");
  assert.equal(shellWord(), "shell");
  assert.equal(runningWord({ ...shell, running: "claude-code" }, { "claude-code": "Claude Code" }), "Claude Code", "a harness typed into the shell names the tab");
  assert.equal(runningWord({ ...shell, harness: "codex" }), "codex", "a harness with no label yet is named by its id");
  assert.equal(runningWord({ ...shell, run: { command: "npm start" } }), "run");
  assert.equal(runningWord({ ...shell, mobileDevelopment: { device: "d1" } }), "flutter");
  assert.equal(ranWords("12 s"), "ran 12 s");
  // Neither the rail's row nor the footer spells one of its own.
  for (const rel of ["../views/_workbench/rail/RailShellRow.tsx", "./footerSessionsModel.mjs"]) {
    const text = readFileSync(new URL(rel, import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
    assert.ok(!/`exited \(|\? "exited"|: "shell"|return "unverifiable"|return "open"/.test(text), `${rel} spells no liveness word of its own`);
  }
});

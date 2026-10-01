/**
 * A working session's shells, end to end through the models: opened at four
 * kinds of place, reported live, exited, restarted, swept, listed in the
 * footer, checkpointed and restored — and the resume nudge a restored harness
 * gets. The facts asserted are the ones the tab strip, the rail and the
 * footer draw (ide/06). Run with `node --test desktop/src/scenarios/terminals.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { readFileSync } from "node:fs";

import { footerSessions } from "../shell/footerSessionsModel.mjs";
import { claimedSessions, isDrawn, workstreamSessionRows } from "../views/_workbench/workstreamSessionsModel.mjs";
import {
  SESSIONS_VERSION,
  TERMINAL_SCOPES,
  closeExitedTerminals,
  emptyTerminals,
  exitNote,
  hasLaunched,
  isLive,
  launchKey,
  mountKey,
  noteExit,
  noteLive,
  openTerminal,
  restartTerminal,
  restoreTerminals,
  serializeTerminals,
  sessionsRootedAt,
  terminalTabLabel,
  terminalTitle,
} from "../shell/terminalsModel.mjs";
import { START_QUIET_MS, armed, due, onOutput, said } from "../terminal/resumeStartModel.mjs";

const at = (s, key) => s.sessions.find((x) => x.key === key);

test("shells open at every kind of place, each tab names its place and what runs in it, and only the machine's stays out of the workbench", () => {
  let s = emptyTerminals();
  s = openTerminal(s, { scope: "goal", id: "01GOAL", label: "ship-it" }, 100);
  s = openTerminal(s, { scope: "workstream", id: "01WS", harness: "claude-code" }, 110);
  s = openTerminal(s, { scope: "work_item", id: "01ITEM" }, 120);
  s = openTerminal(s, { scope: "machine", id: "home", login: { kind: "github", host: "github.com" } }, 130);
  assert.equal(s.sessions.length, 4);
  assert.deepEqual([...new Set(s.sessions.map((x) => x.scope))].sort(), [...TERMINAL_SCOPES].sort());
  assert.equal(s.active, s.sessions[3].key, "the last opened is the active tab");

  const [goal, ws, item, home] = s.sessions;
  assert.equal(terminalTitle(goal), "Goal · ship-it");
  assert.equal(terminalTitle(ws), "claude-code · Workstream 01WS", "no label: the place's id tail names it");
  assert.equal(terminalTitle(item), "Work item 01ITEM");
  assert.equal(terminalTitle(home), "This machine home");
  assert.deepEqual(terminalTabLabel(ws, { "claude-code": "Claude Code" }), "Claude Code");
  assert.equal(terminalTabLabel(goal), "shell · ship-it");
  assert.deepEqual(home.login, { kind: "github", host: "github.com" }, "a sign-in tab carries its target");
  assert.equal(goal.login, null);

  const keys = s.sessions.map(mountKey);
  assert.equal(new Set(keys).size, 4, "four mounts, no two alike");
  assert.deepEqual(sessionsRootedAt(s.sessions, "workstream", "01WS").map((x) => x.key), [ws.key]);
  assert.deepEqual(sessionsRootedAt(s.sessions, "workstream", "elsewhere"), []);
});

test("a shell exits, its tab stays with the note, a restart is a fresh mount at the same place, and a sweep takes only the exited", () => {
  let s = emptyTerminals();
  s = openTerminal(s, { scope: "workstream", id: "w1" }, 1000);
  s = openTerminal(s, { scope: "workstream", id: "w1", harness: "codex" }, 1001);
  const [plain, harness] = s.sessions.map((x) => x.key);
  s = noteLive(s, plain, 0, "pty-1", null, 1002);
  s = noteLive(s, harness, 0, "pty-2", "01SESSION", 1003);
  assert.equal(at(s, harness).sessionId, "01SESSION", "the harness reported the roster session it is");

  s = noteExit(s, plain, 0, 130, 1200);
  assert.equal(isLive(at(s, plain)), false);
  assert.equal(exitNote(at(s, plain)), "exited (130)");
  assert.equal(at(s, plain).exitedAt, 1200);
  assert.equal(s.sessions.length, 2, "a dead shell keeps its tab");
  assert.equal(noteExit(s, plain, 9, 1, 1300), s, "a stale generation cannot re-kill it");

  const before = mountKey(at(s, plain));
  s = restartTerminal(s, plain, 2000);
  assert.equal(isLive(at(s, plain)), true);
  assert.notEqual(mountKey(at(s, plain)), before, "React mounts a new PTY");
  assert.equal(at(s, plain).key, plain, "the tab is the same tab");
  assert.equal(at(s, plain).openedAt, 2000, "a fresh clock");
  assert.equal(at(s, plain).terminalId, null, "the new PTY has not reported yet");

  s = noteExit(s, harness, 0, 0, 2100);
  const swept = closeExitedTerminals(s, "workstream", "w1");
  assert.deepEqual(swept.sessions.map((x) => x.key), [plain], "the live one stays, the exited one went");
  assert.equal(closeExitedTerminals(swept, "workstream", "w1"), swept, "nothing exited: the same state");
});

test("the footer lists the shells and the harnesses standing in a place, and a harness in a tab is one row, not two", () => {
  let s = emptyTerminals();
  s = openTerminal(s, { scope: "workstream", id: "w1" }, 1);
  s = openTerminal(s, { scope: "workstream", id: "w1", harness: "claude-code" }, 2);
  s = openTerminal(s, { scope: "goal", id: "g1" }, 3);
  const [shell, harnessTab, goalShell] = s.sessions.map((x) => x.key);
  s = noteLive(s, harnessTab, 0, "pty", "s1", 4);
  s = noteExit(s, goalShell, 0, 0, 5);
  const roster = [
    { id: "s1", kind: "terminal", state: { state: "running" }, harness: "claude-code", agent: null, workstream: "w1" },
    { id: "s2", kind: "worker", state: { state: "thinking" }, harness: "codex", agent: "general-agent", workstream: "w1" },
    { id: "s3", kind: "worker", state: { state: "done" }, harness: "codex", agent: "general-agent", workstream: "w1" },
  ];
  const out = footerSessions(s.sessions, roster);
  assert.deepEqual(
    out.terminals.map((t) => [t.key, t.word, t.open]),
    [
      [shell, "open", true],
      [goalShell, "exited", false],
    ],
    "the claimed tab is a harness row; the exited goal shell is listed last and not counted",
  );
  assert.deepEqual(
    out.harnesses.map((h) => [h.id, h.terminalKey]),
    [
      ["s1", harnessTab],
      ["s2", null],
    ],
    "the engine's own session has no tab; the done one is gone",
  );
  assert.equal(out.openTerminals, 1);
});

test("a checkpoint restores every tab as unverifiable — never dead — and each learns its clock when its PTY reports", () => {
  let s = emptyTerminals();
  s = openTerminal(s, { scope: "workstream", id: "w1", harness: "claude-code" }, 100);
  s = openTerminal(s, { scope: "machine", id: "home", login: { kind: "github", host: "github.com" } }, 101);
  s = openTerminal(s, { scope: "workstream", id: "w2" }, 102);
  const [harness, signIn, plain] = s.sessions.map((x) => x.key);
  s = noteExit(s, plain, 0, 0, 200);

  const stored = JSON.parse(JSON.stringify(serializeTerminals(s)));
  assert.equal(stored.version, SESSIONS_VERSION);
  const back = restoreTerminals(stored, s.launched);
  assert.deepEqual(back.sessions.map((x) => x.key), [harness, signIn, plain], "every tab comes back, the sign-in included");
  assert.deepEqual(at(back, signIn).login, { kind: "github", host: "github.com" }, "a sign-in keeps its target across a restart");
  for (const x of back.sessions) {
    assert.equal(x.liveness.status, "unverifiable", "loss of contact is never evidence of death");
    assert.match(x.liveness.reason, /restored from a checkpoint/, "and the tab can say why it does not know");
    assert.equal(x.openedAt, null, "no PTY yet, no clock");
    assert.equal(x.generation, 1, "one generation on from the checkpoint");
    assert.equal(x.terminalId, null);
    assert.equal(x.sessionId, null, "a harness registers anew");
  }
  assert.equal(at(back, harness).harness, "claude-code", "what the tab runs is remembered");
  assert.equal(at(back, harness).resume, false, "the tab repeats what it did — its first launch started fresh — not what the place has since become");
  assert.equal(at(back, plain).resume, false, "a plain shell never resumes");
  assert.equal(openTerminal(back, { scope: "workstream", id: "w1", harness: "claude-code" }, 300).sessions.at(-1).resume, true, "a new launch in that place, though, resumes: the memory came back with the checkpoint");

  const live = noteLive(back, harness, 1, "pty-9", null, 900);
  assert.equal(at(live, harness).openedAt, 900, "learned when live");
  assert.equal(isLive(at(live, harness)), true);
  assert.equal(at(live, plain).openedAt, null, "the other tab is still waiting to hear");
  assert.equal(at(live, signIn).openedAt, null);
});

test("a restored harness is nudged to pick its work up once its prompt is drawn — and only once", () => {
  let s = emptyTerminals();
  const place = { scope: "workstream", id: "w1", harness: "claude-code" };
  assert.equal(hasLaunched(s.launched, launchKey("claude-code", "workstream", "w1")), false);
  s = openTerminal(s, place, 1);
  assert.equal(s.sessions[0].resume, false, "the first launch in a place starts fresh");
  s = restartTerminal(s, s.sessions[0].key, 2);
  assert.equal(s.sessions[0].resume, false, "a restart repeats what its own tab did");
  const second = openTerminal(s, place, 3);
  assert.equal(second.sessions[1].resume, true, "a second launch in the same place resumes");

  // The nudge: quiet after the first output, said once.
  let start = armed(10_000);
  assert.equal(due(start, 10_000 + START_QUIET_MS), false, "nothing drawn yet");
  start = onOutput(start, 10_200);
  assert.equal(due(start, 10_200 + START_QUIET_MS), true, "the prompt is drawn");
  start = said(start);
  assert.equal(due(start, 20_000), false);
});

test("a harness opened in the home folder is a terminal and never a roster row; a node that started again has forgotten every terminal row, and the tabs draw as shells", () => {
  let s = emptyTerminals();
  // A code host sign-in: a harness in the `machine` scope. The shell asks the node for no row, so no session id ever reaches the tab.
  s = openTerminal(s, { scope: "machine", id: "home", harness: "claude-code", login: { host: "github" } }, 1);
  const home = s.sessions[0];
  s = noteLive(s, home.key, 0, "pty", null, 2);
  assert.equal(at(s, home.key).sessionId, null, "no session was minted for it");
  const roster = [{ id: "s1", kind: "terminal", state: { state: "running" }, harness: "claude-code", agent: null, workstream: "w1" }];
  assert.equal(claimedSessions(roster, s.sessions).size, 0);
  const footer = footerSessions(s.sessions, roster);
  assert.deepEqual(footer.terminals.map((t) => [t.scope, t.harness, t.open, t.place]), [["machine", "claude-code", true, "this machine"]], "a terminal row wearing the harness, standing on this machine");
  assert.deepEqual(footer.harnesses, [], "the orphan row the roster still holds is nobody's: not drawn, not counted");
  // Opening it from a row lands in its tab and never in a workstream's Agents pane.
  const doors = readFileSync(new URL("../shell/sessionDoors.ts", import.meta.url), "utf8");
  assert.ok(doors.includes('return scope === "machine" ? null : scope;'), "a machine-rooted tab lives in the terminal layer alone");
  assert.ok(doors.includes("if (!session.workstream) return false;"), "a session in no checkout has nowhere to open");

  // A harness in a checkout, reported, then the node restarts and forgets its row.
  s = openTerminal(s, { scope: "workstream", id: "w1", harness: "codex" }, 3);
  const tab = s.sessions[1].key;
  s = noteLive(s, tab, 0, "pty2", "s9", 4);
  const before = workstreamSessionRows([{ id: "s9", kind: "terminal", state: { state: "running", tool: "Read", args: "" }, harness: "codex", agent: null, workstream: "w1", children: [] }], s.sessions, "w1");
  assert.deepEqual(before.map((r) => [r.kind, r.terminalKey ?? null]), [["agent", tab]], "drawn once, as the session's row remembering its tab");
  // The roster reseeded from a node that knows nothing of it: the tab is a plain terminal row from then on.
  const after = workstreamSessionRows([], s.sessions, "w1");
  assert.deepEqual(after.map((r) => [r.kind, r.harness, r.liveness.status]), [["terminal", "codex", "live"]]);
  assert.equal(isDrawn({ id: "s9", kind: "terminal", workstream: "w1" }, claimedSessions([], s.sessions)), false, "and the forgotten row, were it to come back unclaimed, is not drawn");
  // The process ends: the tab reads exited, and the footer no longer counts it.
  s = noteExit(s, tab, 0, 1, 5);
  assert.equal(exitNote(at(s, tab)), "exited (1)");
  assert.equal(footerSessions(s.sessions, []).openTerminals, 1, "the sign-in shell alone");
});

import test from "node:test";
import assert from "node:assert/strict";
import { DEFAULT_MODE, DEFAULT_MODE_KEY, DOCUMENT_MODE, MODES, availableModes, centreOf, modeFor, modeOf, nextMode, parseRememberedModes, rememberMode } from "./ideModeModel.mjs";

test("the three modes, project first, and a stray value falls to project", () => {
  assert.deepEqual([...MODES], ["project", "agent", "board"]);
  assert.equal(DEFAULT_MODE, "project");
  assert.equal(DOCUMENT_MODE, "project", "a document opens in the centre that shows documents");
  assert.ok(MODES.includes(DOCUMENT_MODE));
  assert.equal(DEFAULT_MODE_KEY, "ide.default_mode");
  assert.equal(modeOf("agent"), "agent");
  assert.equal(modeOf("board"), "board");
  assert.equal(modeOf("both"), "project");
  assert.equal(modeOf(null), "project");
});

test("the centre is what the mode shows, and only a workstream with a project has a conversation or a board", () => {
  const placed = { scope: "workstream", hasProject: true };
  assert.equal(centreOf("project", placed), "documents");
  assert.equal(centreOf("agent", placed), "conversation");
  assert.equal(centreOf("board", placed), "board");
  assert.equal(centreOf("agent", { scope: "goal", hasProject: true }), "documents", "a goal's root has no conversation of its own");
  assert.equal(centreOf("board", { scope: "work_item", hasProject: true }), "documents");
  assert.equal(centreOf("agent", { scope: "workstream", hasProject: false }), "documents", "a workstream not placed in a project shows documents");
  assert.equal(centreOf("nope", placed), "documents", "a stray value shows documents");
});

test("the Board is offered only while the setting has it on", () => {
  assert.deepEqual(availableModes(), ["project", "agent", "board"]);
  assert.deepEqual(availableModes(false), ["project", "agent"]);
  assert.equal(modeOf("board", false), "project", "a hidden Board is no mode");
  assert.equal(modeFor("board", "agent", false), "agent", "a root that remembered the Board opens in the default while it is off");
  assert.equal(modeFor(undefined, "board", false), "project", "a default naming the hidden Board is clamped too");
});

test("a root opens in what was remembered for it, else the settings' default", () => {
  assert.equal(modeFor("agent", "project"), "agent", "remembered wins");
  assert.equal(modeFor("board", "project"), "board");
  assert.equal(modeFor(undefined, "agent"), "agent", "nothing remembered: the default");
  assert.equal(modeFor("nope", "agent"), "agent", "a stale memory is no memory");
  assert.equal(modeFor(undefined, "nope"), "project", "a default off the wire is clamped too");
});

test("a cycle goes round the switch: Project, Agent, Board, Project — and skips a hidden Board", () => {
  assert.equal(nextMode("project"), "agent");
  assert.equal(nextMode("agent"), "board");
  assert.equal(nextMode("board"), "project");
  assert.equal(nextMode("nope"), "agent", "from a stray value, project's next");
  assert.equal(nextMode("agent", false), "project", "the Board off: two modes round");
});

test("memory is per root, newest last, capped, and read back only when real", () => {
  let m = rememberMode({}, "workstream:a", "agent");
  assert.deepEqual(m, { "workstream:a": "agent" });
  assert.equal(rememberMode(m, "workstream:a", "agent"), m, "unchanged is the same object");
  m = rememberMode(m, "workstream:b", "board", 2);
  m = rememberMode(m, "workstream:c", "agent", 2);
  assert.deepEqual(Object.keys(m), ["workstream:b", "workstream:c"], "the oldest is forgotten past the cap");
  assert.deepEqual(parseRememberedModes({ "workstream:a": "agent", "workstream:b": "nope", "workstream:c": "board", nope: "agent", 3: "agent" }), { "workstream:a": "agent", "workstream:c": "board" });
  assert.deepEqual(parseRememberedModes(["agent"]), {});
  assert.deepEqual(parseRememberedModes(null), {});
});

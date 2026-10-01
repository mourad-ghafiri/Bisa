/**
 * The workflow designer as a person edits one (03-workflows §The designer):
 * a stored workflow opens clean, an added step and a connection are one edit
 * each on the graph the canvas draws, the save is due after the quiet delay
 * and never later than the max wait, the save sends the base's revision and
 * moves with the answer, undo never lies about what is saved, our own echo
 * on the bus is not a reload, a conflict is a choice, and every status line
 * is the person's words. No DOM: the canvas renders nothing here.
 *
 * Run with `node --test desktop/src/scenarios/designer.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { MAX_QUIET_MULTIPLE, canRedoEdit, canUndoEdit, conflict, dirty, edit, keepMine, open, present, redoEdit, remoteDecision, saveDue, saveRequest, saveStarted, saveSucceeded, statusLine, takeTheirs, undoEdit } from "../views/_workflow/designerSession.mjs";
import { addStep, connect, removeStep, renameStep, setPositions, toGraph } from "../views/_workflow/workflowGraph.mjs";
import { GRID, NODE_HEIGHT, layout, nudgeFree, snapTo, tidy, withPositions } from "../views/_workflow/workflowLayout.mjs";
import { paneForSelection, panelTabs, pressPane } from "../views/_workflow/designerPanelModel.mjs";

const stored = (revision, name = "Ship it") => ({
  id: "01WF",
  name,
  description: "",
  inputs: [],
  steps: [{ id: "a", name: "A", kind: "approval", prompt: "?" }],
  origin: { origin: "workspace" },
  author: "ab".repeat(32),
  tags: [],
  revision,
  created_at: 0,
});
const DELAY = 800;

test("open, add a step, connect it, save — the graph, the due save, the revision and the status at every step", () => {
  let s = open(stored(1));
  assert.equal(dirty(s), false);
  assert.equal(statusLine(s), "saved");
  assert.equal(saveDue(s, 1000, DELAY), null, "clean: nothing is due");
  assert.deepEqual(toGraph(present(s)).nodes.map((n) => n.id), ["a"]);

  // Add an agent step: one edit, one new node, no edge yet.
  const added = addStep(present(s), "agent");
  assert.equal(added.id, "agent");
  s = edit(s, added.wf, 1000);
  assert.equal(dirty(s), true);
  assert.equal(statusLine(s), "unsaved edits");
  assert.deepEqual(toGraph(present(s)).nodes.map((n) => n.id), ["a", "agent"]);
  assert.deepEqual(toGraph(present(s)).edges, []);
  assert.equal(saveDue(s, 1000, DELAY), DELAY, "the quiet delay from the edit");
  assert.equal(saveDue(s, 1500, DELAY), 300, "measured from the edit, not from now");

  // Connect a → agent: legal; a → a and a flow out of nothing are refused with the reason the canvas says.
  const wired = connect(present(s), "a", "agent");
  assert.equal(wired.ok, true);
  assert.equal(connect(present(s), "a", "a").reason, "a step cannot flow into itself");
  assert.equal(connect(present(s), "a", "nope").reason, "both ends must be steps");
  s = edit(s, wired.wf, 1200);
  assert.deepEqual(toGraph(present(s)).edges.map((e) => [e.from, e.to, e.loop]), [["a", "agent", false]]);
  assert.equal(connect(present(s), "a", "agent").reason, "that flow already exists");
  // Typing keeps the save quiet, but never past the max wait.
  assert.equal(saveDue(s, 1200, DELAY), DELAY, "the quiet delay restarts from the last edit");
  assert.equal(saveDue(s, 1000 + MAX_QUIET_MULTIPLE * DELAY + 1, DELAY), 0, "ten delays after the first edit it goes now");

  // The save carries the base's revision; in flight nothing more is due; the answer moves the base.
  const req = saveRequest(s);
  assert.equal(req.kind, "update");
  assert.equal(req.revision, 1);
  assert.deepEqual(req.body.steps.map((x) => x.id), ["a", "agent"]);
  s = saveStarted(s, req.body);
  assert.equal(statusLine(s), "saving…");
  assert.equal(saveDue(s, 5000, DELAY), null, "in flight: nothing more is due");
  s = saveSucceeded(s, { ...stored(2), ...req.body });
  assert.equal(s.base.revision, 2);
  assert.equal(dirty(s), false);
  assert.equal(statusLine(s), "saved");
  // Our own echo is not a reload; a stranger's revision is worth a look.
  assert.equal(remoteDecision(s, 2), "ignore");
  assert.equal(remoteDecision(s, 3), "reload");
});

test("undo never lies: undoing past a save is dirty again against the saved base, redo lands back on it, and a rename rewrites every reference", () => {
  let s = open(stored(1));
  const added = addStep(present(s), "agent");
  s = edit(s, connect(added.wf, "a", "agent").wf, 1000);
  const req = saveRequest(s);
  s = saveSucceeded(saveStarted(s, req.body), { ...stored(2), ...req.body });
  assert.equal(canUndoEdit(s), true);
  s = undoEdit(s);
  assert.deepEqual(toGraph(present(s)).nodes.map((n) => n.id), ["a"], "back before the step");
  assert.equal(dirty(s), true, "what is on screen is not what is saved");
  assert.equal(saveRequest(s).revision, 2, "against the saved base, not the body's age");
  assert.equal(canRedoEdit(s), true);
  s = redoEdit(s);
  assert.equal(dirty(s), false, "redo lands on what was saved");
  // A rename moves the id and every flow that named it.
  const renamed = renameStep(present(s), "agent", "build");
  assert.ok(renamed.ok !== false, "a free name is taken");
  const wf = renamed.wf ?? renamed;
  assert.deepEqual(toGraph(wf).edges.map((e) => e.to), ["build"]);
  // Removing the step takes its flows with it.
  const removed = removeStep(wf, "build");
  const left = removed.wf ?? removed;
  assert.deepEqual(toGraph(left).nodes.map((n) => n.id), ["a"]);
  assert.deepEqual(toGraph(left).edges, []);
});

test("a conflict is a choice: keep mine sends again over the new base, take theirs drops the edit and is clean", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Mine" }, 1000);
  const req = saveRequest(s);
  s = saveStarted(s, req.body);
  // The node refused: somebody saved revision 2 meanwhile.
  const theirs = stored(2, "Theirs");
  s = conflict(s, theirs);
  assert.match(statusLine(s), /conflict|changed|else/i);
  const mine = keepMine(s);
  assert.equal(present(mine).name, "Mine");
  assert.equal(saveRequest(mine).revision, 2, "sent again over the new base");
  const taken = takeTheirs(s);
  assert.equal(present(taken).name, "Theirs");
  assert.equal(dirty(taken), false);
  assert.equal(statusLine(taken), "saved");
});

test("the picture is the person's: a drop lands where it was released and stays after a connect; the first edit writes every place; tidy is one edit", () => {
  // A stored workflow nobody placed: the canvas draws the layered picture.
  let s = open({ ...stored(1), steps: [{ id: "a", name: "A", kind: "approval", prompt: "?", then: [{ to: "b" }] }, { id: "b", name: "B", kind: "end" }] });
  let drawn = layout(present(s)).positions;
  assert.equal(present(s).steps.some((st) => st.position), false, "nothing written yet");
  assert.equal(drawn.get("b").y, drawn.get("a").y + NODE_HEIGHT + 72, "b under a, the layered way");

  // A palette drop, the way the canvas commits it: the step is added where
  // it was released, and every derived position is written with it.
  const at = nudgeFree(drawn, null, "agent", snapTo({ x: 500, y: 13 }, GRID));
  const added = addStep(present(s), "agent", undefined, at);
  s = edit(s, withPositions(added.wf, drawn), 1000);
  const body = present(s);
  assert.deepEqual(body.steps.find((st) => st.id === "agent").position, { x: 504, y: 16 }, "on the grid, where it was dropped");
  assert.deepEqual(body.steps.find((st) => st.id === "a").position, drawn.get("a"), "the first canvas edit writes the derived places too");
  assert.deepEqual(body.steps.find((st) => st.id === "b").position, drawn.get("b"));

  // Connecting a → agent moves nothing.
  const before = layout(present(s)).positions;
  const wired = connect(present(s), "a", "agent");
  assert.equal(wired.ok, true);
  s = edit(s, withPositions(wired.wf, before), 1100);
  const after = layout(present(s)).positions;
  assert.deepEqual([...after.entries()], [...before.entries()], "a flow changes no card's place");

  // A move is one edit; undo is one step back to the previous picture.
  s = edit(s, setPositions(present(s), new Map([["b", { x: 800, y: 800 }]])), 1200);
  assert.deepEqual(layout(present(s)).positions.get("b"), { x: 800, y: 800 });
  s = undoEdit(s);
  assert.deepEqual(layout(present(s)).positions.get("b"), before.get("b"), "one undo, the card is back");
  s = redoEdit(s);

  // Tidy: one edit, every card placed again, the list in reading order; undo brings the person's picture back.
  const tidied = tidy(present(s));
  assert.deepEqual(tidied.steps.map((st) => st.id), ["a", "b", "agent"], "steps read top to bottom, rank-mates in their order");
  assert.ok(tidied.steps.every((st) => st.position));
  s = edit(s, tidied, 1300);
  assert.notDeepEqual(layout(present(s)).positions.get("b"), { x: 800, y: 800 });
  s = undoEdit(s);
  assert.deepEqual(layout(present(s)).positions.get("b"), { x: 800, y: 800 }, "undo after tidy is the person's picture");
  assert.equal(canRedoEdit(s), true);
});

test("the right panel: a workflow exists from its first second, so Agent is never muted; a step picked while chatting shows its properties, and the Agent tab closes the column on a second press", () => {
  // *New workflow* created the workflow on the node before the designer
  // opened: the Agent pane is there to press at once.
  let panel = { open: true, tab: "agent" };
  const tabs = panelTabs({ open: panel.open, tab: panel.tab });
  assert.deepEqual(tabs.map((t) => [t.id, t.showing]), [["properties", false], ["agent", true], ["runs", false]]);
  assert.ok(tabs.every((t) => !("muted" in t)), "no tab is ever muted");
  const library = readFileSync(new URL("../views/Workflows.tsx", import.meta.url), "utf8");
  assert.ok(library.includes("api.createWorkflow(definitionBody(blankWorkflow()))") && !library.includes("workflow_new"), "the library creates, then opens the id");
  const designer = readFileSync(new URL("../views/WorkflowDesigner.tsx", import.meta.url), "utf8");
  assert.ok(!designer.includes("openDraft") && !designer.includes("draftHandoff") && designer.includes("id: string }"), "the designer opens a stored workflow only");
  // A step clicked on the canvas while the conversation shows: Properties.
  panel = paneForSelection(panel, "build");
  assert.deepEqual(panel, { open: true, tab: "properties" });
  // The Agent tab pressed: the conversation again; pressed once more: the column closes, the canvas has the room.
  panel = pressPane(panel, "agent");
  assert.deepEqual(panel, { open: true, tab: "agent" });
  panel = pressPane(panel, "agent");
  assert.deepEqual(panel, { open: false, tab: "agent" });
  assert.deepEqual(panelTabs(panel).map((t) => t.showing), [false, false, false], "a closed column marks no tab");
  // A step picked with the column closed opens it on Properties.
  assert.deepEqual(paneForSelection(panel, "ship"), { open: true, tab: "properties" });
  // The Runs tab: the workflow's runs of the workspace, beside the other two;
  // a second press closes the column like any tab.
  panel = pressPane({ open: true, tab: "properties" }, "runs");
  assert.deepEqual(panel, { open: true, tab: "runs" });
  assert.deepEqual(panelTabs(panel).map((t) => [t.id, t.showing]), [["properties", false], ["agent", false], ["runs", true]]);
  assert.deepEqual(pressPane(panel, "runs"), { open: false, tab: "runs" });
  assert.ok(designer.includes("<WorkflowRunsPane"), "the designer draws the Runs pane");
});

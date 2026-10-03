/**
 * The designer's document, tested where it lives: a save never loses a
 * keystroke, undo never resends a stale revision, a conflict is a choice.
 *
 * Run with `npm test` from `desktop/`.
 */

import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { libraryReads } from "./libraryModel.mjs";
import { MAX_QUIET_MULTIPLE, RETRY_CAP_MS, asksStored, refusalOutcome, bodyOf, canUndoEdit, caughtUp, conflict, definitionBody, dirty, edit, keepMine, open, present, problemsFromErrorBody, putBody, redoEdit, remoteDecision, remoteLoaded, retryDelayMs, saveDue, saveFailed, saveRejected, saveRequest, saveStarted, saveSucceeded, settle, statusLine, takeTheirs, undoEdit, withProblems, WORKFLOW_FACTS, goalTabRemote, remoteAction, rowBehind } from "./designerSession.mjs";

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

/** Drive one save to completion: request → started → node stored `saved`. */
function saved(s, revisionAfter) {
  const req = saveRequest(s);
  assert.ok(req, "something to save");
  s = saveStarted(s, req.body);
  return saveSucceeded(s, { ...stored(revisionAfter), ...req.body });
}

test("bodyOf carries the decision-making switch through, defaulting to off", () => {
  assert.equal(bodyOf(stored(1)).decision_making, false);
  assert.equal(bodyOf({ ...stored(1), decision_making: true }).decision_making, true);
});

test("bodyOf reads the switch under its one name: the retired field switches nothing on and is not carried", () => {
  const body = bodyOf({ ...stored(1), decision_maker: true }); // terminology-lint-ignore: decision-maker - proves the retired word is refused
  assert.equal(body.decision_making, false);
  assert.deepEqual(Object.keys(body), ["name", "description", "inputs", "steps", "tags", "decision_making"]);
});

test("what leaves for the node is the definition by name: a key the definition does not have stays on the canvas, and a save adds the revision alone", () => {
  const steps = [{ id: "a", name: "A", kind: "approval", prompt: "?" }];
  const inputs = [{ name: "who", label: "Who", kind: "text" }];
  const body = { name: "Ship it", description: "how we ship", inputs, steps, tags: ["delivery"], decision_making: true };
  // The ordinary case: the six keys, as they stand.
  assert.deepEqual(definitionBody(body), { name: "Ship it", description: "how we ship", inputs, steps, tags: ["delivery"], decision_making: true });
  assert.deepEqual(putBody(body, 4), { name: "Ship it", description: "how we ship", inputs, steps, tags: ["delivery"], decision_making: true, revision: 4 });
  // A drawing that carries more — a stored workflow's record fields, the canvas's own, another version's.
  const drawn = { ...stored(3), ...body, selected: "a", viewport: { x: 0, y: 0, zoom: 1 }, decisions: true, past: [] };
  assert.deepEqual(definitionBody(drawn), definitionBody(body));
  assert.deepEqual(Object.keys(putBody(drawn, 3)), ["name", "description", "inputs", "steps", "tags", "decision_making", "revision"]);
  // What is absent is the definition's own default, never a key left undefined.
  assert.deepEqual(definitionBody({ name: "Bare", steps }), { name: "Bare", description: "", inputs: [], steps, tags: [], decision_making: false });
  // The steps and the inputs are the canvas's own objects; the tags are a copy.
  assert.equal(definitionBody(body).steps, steps);
  assert.notEqual(definitionBody(body).tags, body.tags);
});

test("opening a stored workflow is clean at its revision, and every save is an update — a workflow exists before its designer opens", () => {
  const s = open(stored(1), []);
  assert.equal(dirty(s), false);
  assert.equal(saveRequest(s), null);
  assert.equal(statusLine(s), "Saved");
  assert.deepEqual(present(s), bodyOf(stored(1)));
  // An empty draft the library just created: stored at revision 1 with its
  // one problem, and its first edit is an update like any other.
  const empty = open({ ...stored(1), steps: [] }, [{ kind: "no_start", message: "empty" }]);
  assert.equal(empty.problems[0].kind, "no_start", "a draft keeps its problems");
  const e = edit(empty, { ...present(empty), name: "Named" });
  assert.deepEqual(saveRequest(e), { kind: "update", id: "01WF", revision: 1, body: present(e) });
});

test("a save sends the head at the base revision, and the revision moves with the base", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Renamed" });
  const req = saveRequest(s);
  assert.equal(req.kind, "update");
  assert.equal(req.revision, 1);
  assert.equal(req.body.name, "Renamed");
  s = saved(s, 2);
  assert.equal(s.base.revision, 2);
  assert.equal(dirty(s), false);
  assert.equal(statusLine(s), "Saved");
});

test("our own save echo is not a reload", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Renamed" });
  const req = saveRequest(s);
  s = saveStarted(s, req.body);
  // The bus hears our own save while it is in flight: ignored.
  assert.equal(remoteDecision(s, 2), "ignore");
  s = saveSucceeded(s, { ...stored(2), ...req.body });
  // And once it landed, the echo names our revision: still ignored.
  assert.equal(remoteDecision(s, 2), "ignore");
  assert.equal(remoteDecision(s, 1), "ignore", "old news");
  assert.equal(remoteDecision(s, 3), "reload", "somebody else's revision is worth a look");
});

test("undo after a save keeps the base revision", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Two" });
  s = saved(s, 2);
  s = edit(s, { ...present(s), name: "Three" });
  s = saved(s, 3);
  s = undoEdit(s);
  assert.equal(present(s).name, "Two");
  const req = saveRequest(s);
  assert.equal(req.revision, 3, "the revision is the node's, not the body's age");
  assert.equal(dirty(s), true);
  s = redoEdit(s);
  assert.equal(dirty(s), false, "back at what was saved");
});

test("typing during a save is preserved and stays unsaved", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Two" });
  const req = saveRequest(s);
  s = saveStarted(s, req.body);
  assert.equal(saveRequest(s), null, "one save at a time");
  s = edit(s, { ...present(s), description: "typed meanwhile" });
  s = saveSucceeded(s, { ...stored(2), ...req.body });
  assert.equal(present(s).description, "typed meanwhile", "the keystroke survived");
  assert.equal(dirty(s), true);
  const next = saveRequest(s);
  assert.equal(next.revision, 2);
  assert.equal(next.body.description, "typed meanwhile");
  assert.ok(canUndoEdit(s));
});

test("a failed save re-arms with a growing delay and the draft where it was", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Two" });
  s = saveStarted(s, saveRequest(s).body);
  s = saveFailed(s, "node unreachable");
  assert.equal(s.status, "failed");
  assert.equal(statusLine(s), "Not saved: node unreachable");
  assert.equal(present(s).name, "Two", "nothing lost");
  assert.ok(saveRequest(s), "still to be sent");
  assert.equal(retryDelayMs(s), 1000);
  s = saveFailed(saveStarted(s, saveRequest(s).body), "again");
  assert.equal(retryDelayMs(s), 2000);
  for (let i = 0; i < 10; i++) s = saveFailed(saveStarted(s, saveRequest(s).body), "again");
  assert.equal(retryDelayMs(s), RETRY_CAP_MS, "capped");
  s = saved(s, 2);
  assert.equal(retryDelayMs(s), 0);
  assert.equal(s.failures, 0);
});

test("a conflict blocks saving and has two exits", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Mine" });
  const req = saveRequest(s);
  s = saveStarted(s, req.body);
  const theirs = stored(2, "Theirs");
  s = conflict(s, theirs);
  assert.equal(s.status, "conflict");
  assert.equal(saveRequest(s), null, "blocked until the person decides");
  assert.equal(remoteDecision(s, 3), "ignore", "no reload over an open conflict");
  assert.equal(present(s).name, "Mine", "my body is still the head");
  assert.ok(statusLine(s).startsWith("Conflict"));

  // Keep mine: my head rides on their revision.
  const mine = keepMine(s);
  assert.equal(mine.status, "idle");
  assert.equal(mine.base.revision, 2);
  assert.equal(present(mine).name, "Mine");
  const next = saveRequest(mine);
  assert.equal(next.revision, 2);
  assert.equal(next.body.name, "Mine");

  // Take theirs: their body is the head, and undo brings mine back.
  const theirsTaken = takeTheirs(s);
  assert.equal(theirsTaken.status, "idle");
  assert.equal(present(theirsTaken).name, "Theirs");
  assert.equal(dirty(theirsTaken), false);
  assert.equal(saveRequest(theirsTaken), null);
  const back = undoEdit(theirsTaken);
  assert.equal(present(back).name, "Mine");
  assert.equal(dirty(back), true, "my fork, unsaved against their revision");
  assert.equal(saveRequest(back).revision, 2);
});

test("a remote revision is adopted when clean and is a conflict when dirty", () => {
  const clean = open(stored(1));
  const adopted = remoteLoaded(clean, stored(2, "Theirs"), []);
  assert.equal(adopted.status, "idle");
  assert.equal(adopted.base.revision, 2);
  assert.equal(present(adopted).name, "Theirs");
  assert.equal(dirty(adopted), false);
  assert.ok(canUndoEdit(adopted), "the previous body is one undo away");

  let d = open(stored(1));
  d = edit(d, { ...present(d), name: "Mine" });
  const clashed = remoteLoaded(d, stored(2, "Theirs"));
  assert.equal(clashed.status, "conflict");
  assert.equal(clashed.conflict.theirs.revision, 2);
  assert.equal(present(clashed).name, "Mine");

  // Not newer after all: nothing changes.
  assert.equal(remoteLoaded(d, stored(1, "Same")), d);
});

test("problems ride along with a save", () => {
  let s = open(stored(1), [{ kind: "no_start", message: "m" }]);
  assert.equal(s.problems.length, 1);
  s = withProblems(s, []);
  assert.deepEqual(s.problems, []);
  s = edit(s, { ...present(s), name: "Two" });
  const req = saveRequest(s);
  s = saveStarted(s, req.body);
  s = saveSucceeded(s, { ...stored(2), ...req.body }, [{ kind: "unused_input", message: "x" }]);
  assert.equal(s.problems[0].kind, "unused_input");
  assert.deepEqual(problemsFromErrorBody({ error: "e", problems: [{ kind: "no_start", message: "m" }] }), [
    { kind: "no_start", message: "m" },
  ]);
  assert.deepEqual(problemsFromErrorBody({ error: "e" }), []);
  assert.deepEqual(problemsFromErrorBody("prose"), []);
  assert.deepEqual(problemsFromErrorBody(undefined), []);
});

test("a save of an empty draft keeps its problems beside the new revision, and the next save updates that revision", () => {
  let d = open({ ...stored(1), steps: [] }, [{ kind: "no_start", message: "empty" }]);
  d = edit(d, { ...present(d), name: "Named" });
  const req = saveRequest(d);
  assert.equal(req.kind, "update");
  d = saveStarted(d, req.body);
  d = saveSucceeded(d, { ...stored(2), ...req.body }, [{ kind: "no_start", message: "empty" }]);
  assert.equal(d.base.id, "01WF");
  assert.equal(dirty(d), false);
  assert.equal(d.problems[0].kind, "no_start", "a draft keeps its problems");
  d = edit(d, { ...present(d), description: "d" });
  const next = saveRequest(d);
  assert.equal(next.kind, "update");
  assert.equal(next.revision, 2);
});

test("a body the node could not read is not sent again; an edit is", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Two" });
  const req = saveRequest(s);
  s = saveStarted(s, req.body);
  s = saveRejected(s, 'steps[0]: invalid connector id ""');
  assert.equal(s.status, "rejected");
  assert.equal(s.rejected, req.body, "the refused body is remembered by reference");
  assert.equal(saveRequest(s), null, "the same body is not retried");
  assert.equal(saveDue(s, 0, 800), null, "and nothing is due");
  assert.equal(retryDelayMs(s), 0, "no backoff: it is not a failure that passes on its own");
  assert.equal(statusLine(s), 'Not saved — the node could not read this design: steps[0]: invalid connector id ""');
  assert.equal(present(s).name, "Two", "nothing lost");
  assert.ok(dirty(s), "still unsaved");
  // An edit is a different body: it may be read.
  s = edit(s, { ...present(s), name: "Three" }, 5000);
  assert.equal(s.status, "rejected", "the status stays until the next save leaves");
  const again = saveRequest(s);
  assert.ok(again, "the edited body is sent");
  assert.equal(again.body.name, "Three");
  s = saveStarted(s, again.body);
  assert.equal(s.rejected, null);
  assert.equal(s.status, "saving");
});

test("a save is due after the quiet delay, or the max wait since the first unsaved edit", () => {
  const delay = 800;
  let s = open(stored(1));
  assert.equal(saveDue(s, 1000, delay), null, "clean: nothing is due");
  s = edit(s, { ...present(s), name: "T" }, 1000);
  assert.equal(saveDue(s, 1000, delay), 800, "the quiet delay from the edit");
  assert.equal(saveDue(s, 1500, delay), 300, "measured from the edit, not from now");
  // Typing without pause: every edit pushes the quiet delay out…
  for (let i = 1; i <= 20; i++) s = edit(s, { ...present(s), name: "T".repeat(i + 1) }, 1000 + i * 500);
  assert.equal(s.editedAt, 11000);
  assert.equal(s.dirtySince, 1000, "the first unsaved edit starts the clock");
  // …but the max wait does not move.
  assert.equal(saveDue(s, 11000, delay), Math.max(0, 1000 + MAX_QUIET_MULTIPLE * delay - 11000));
  assert.equal(saveDue(s, 11000, delay), 0, "ten delays after the first edit, it goes now");
  // The save leaves: what is typed from now on is the next one's.
  const req = saveRequest(s);
  s = saveStarted(s, req.body);
  assert.equal(s.dirtySince, null);
  assert.equal(saveDue(s, 11000, delay), null, "in flight: nothing more is due");
  s = edit(s, { ...present(s), name: "typed meanwhile" }, 11200);
  assert.equal(s.dirtySince, 11200, "the edit during the save starts the next clock");
  s = saveSucceeded(s, { ...stored(2), ...req.body });
  assert.ok(dirty(s), "the edit typed meanwhile is still unsaved");
  assert.equal(saveDue(s, 11200, delay), 800);
  // After a failure the backoff decides, not the quiet.
  s = saveStarted(s, saveRequest(s).body);
  s = saveFailed(s, "unreachable");
  assert.equal(saveDue(s, 99999, delay), retryDelayMs(s));
  assert.equal(saveDue(s, 99999, delay), 1000);
});

test("undo and redo move the head like an edit, so the save follows them", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Two" }, 100);
  s = saved(s, 2);
  assert.equal(saveDue(s, 100, 800), null, "clean after the save");
  s = undoEdit(s, 2000);
  assert.ok(dirty(s), "undo past the saved body is an unsaved edit");
  assert.equal(s.editedAt, 2000);
  assert.equal(saveDue(s, 2000, 800), 800);
  s = redoEdit(s, 2500);
  assert.equal(dirty(s), false, "redo back to the saved body is clean");
  assert.equal(saveDue(s, 2500, 800), null);
});

test("settle is the one place a save's outcome becomes a session", () => {
  let s = open(stored(1));
  s = edit(s, { ...present(s), name: "Two" });
  const req = saveRequest(s);
  const inFlight = saveStarted(s, req.body);
  const ok = settle(inFlight, { kind: "stored", workflow: { ...stored(2), ...req.body }, problems: [{ kind: "no_start", message: "x" }] });
  assert.equal(ok.status, "idle");
  assert.equal(ok.base.revision, 2);
  assert.equal(ok.problems[0].kind, "no_start");
  const theirs = stored(3, "Theirs");
  assert.equal(settle(inFlight, { kind: "conflict", theirs }).status, "conflict");
  const rejected = settle(inFlight, { kind: "rejected", message: "unreadable" });
  assert.equal(rejected.status, "rejected");
  assert.equal(rejected.rejected, req.body);
  const failed = settle(inFlight, { kind: "failed", message: "down", problems: [] });
  assert.equal(failed.status, "failed");
  assert.equal(failed.failures, 1);
  const withList = settle(inFlight, { kind: "failed", message: "refused", problems: [{ kind: "empty_name", message: "n" }] });
  assert.equal(withList.problems[0].kind, "empty_name", "a refusal that carried problems shows them");
  assert.throws(() => settle(inFlight, { kind: "nonsense" }), /not a save outcome/);
});

test("a fact about workflows means leave, remark, reload or nothing — and nothing for any workflow but the one open", () => {
  const s = { base: { id: "wf-1", revision: 4 }, status: "idle" };
  assert.equal(remoteAction(s, { type: "workflow_deleted", workflow: "wf-1" }), "leave");
  assert.equal(remoteAction(s, { type: "workflow_archived", workflow: "wf-1" }), "remark");
  assert.equal(remoteAction(s, { type: "workflow_changed", workflow: "wf-1", revision: 5 }), "reload");
  assert.equal(remoteAction(s, { type: "workflow_changed", workflow: "wf-1", revision: 4 }), "ignore", "our own save, echoed back");
  assert.equal(remoteAction({ ...s, status: "saving" }, { type: "workflow_changed", workflow: "wf-1", revision: 9 }), "ignore", "nothing arriving while we save can be judged yet");
  assert.equal(remoteAction({ ...s, status: "conflict" }, { type: "workflow_changed", workflow: "wf-1", revision: 9 }), "ignore", "a conflict is already the person's to settle");
  for (const type of WORKFLOW_FACTS) assert.equal(remoteAction(s, { type, workflow: "wf-2", revision: 99 }), "ignore", `${type} of another workflow`);
  assert.equal(remoteAction(s, { type: "run_finished", workflow: "wf-1" }), "ignore");
  for (const none of [null, undefined, {}]) assert.equal(remoteAction(none, { type: "workflow_deleted", workflow: "wf-1" }), "ignore", "no session open: nothing to leave");
  assert.equal(remoteAction(s, null), "ignore");
});

test("a goal's Workflow tab reads a moved workflow again only when nothing is drawn: a drawing keeps its stale revision so its save is refused, never written over the other change", () => {
  const changed = { type: "workflow_changed", workflow: "wf-1", revision: 7 };
  assert.equal(goalTabRemote({ workflow: "wf-1", drawing: false, running: false }, changed), "reload");
  assert.equal(goalTabRemote({ workflow: "wf-1", drawing: true, running: false }, changed), "warn");
  assert.equal(goalTabRemote({ workflow: "wf-1", drawing: false, running: true }, changed), "ignore", "a run's copy is frozen");
  for (const type of WORKFLOW_FACTS) assert.equal(goalTabRemote({ workflow: "wf-1", drawing: false, running: false }, { type, workflow: "wf-1" }), "reload", type);
  assert.equal(goalTabRemote({ workflow: "wf-1", drawing: false, running: false }, { ...changed, workflow: "wf-2" }), "ignore");
  assert.equal(goalTabRemote({ workflow: null, drawing: true, running: false }, changed), "ignore", "a goal with no workflow yet hears none");
  assert.equal(goalTabRemote({ workflow: "wf-1", drawing: false, running: false }, { type: "step_changed", workflow: "wf-1" }), "ignore");
  assert.equal(goalTabRemote(null, changed), "ignore");
});

test("a session opened on the answer kept from the last visit stands on the node's own once it lands", () => {
  // Drawn at once from what the window kept: revision 3. The node has 5.
  const kept = open(stored(3, "As it was left"), []);
  const fresh = caughtUp(kept, stored(5, "As the node has it"), [{ kind: "empty_name", message: "named nothing" }]);
  assert.equal(fresh.base.revision, 5);
  assert.equal(present(fresh).name, "As the node has it");
  assert.equal(dirty(fresh), false);
  assert.equal(canUndoEdit(fresh), false, "opened again, whole: undo never walks back to the stale drawing");
  assert.equal(fresh.problems[0].kind, "empty_name", "with the node's problems");
  // Nothing moved while the person was away: the very same session.
  assert.equal(caughtUp(kept, stored(3, "As it was left")), kept);
  assert.equal(caughtUp(kept, stored(2)), kept, "an older answer is old news");
});

test("an edit made before the node's answer landed is never lost to it: a newer revision is a conflict to settle", () => {
  let s = open(stored(3));
  s = edit(s, { ...present(s), name: "Typed at once" });
  const met = caughtUp(s, stored(5, "Theirs"));
  assert.equal(met.status, "conflict");
  assert.equal(met.conflict.theirs.revision, 5);
  assert.equal(present(met).name, "Typed at once", "the keystroke is still there");
  assert.equal(saveRequest(met), null, "and nothing is sent until the person picks");
  // The node had nothing newer: the edit saves at the revision it was made on.
  const same = caughtUp(s, stored(3));
  assert.equal(same, s);
  assert.equal(saveRequest(same).revision, 3);
});

test("every surface that shows a stored workflow hears it move", () => {
  const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");
  assert.ok(read("../WorkflowDesigner.tsx").includes("remoteAction(session, e.payload)"));
  assert.ok(read("./GoalWorkflowTab.tsx").includes("goalTabRemote("));
  assert.ok(read("./WorkflowPicker.tsx").includes("WORKFLOW_FACTS.includes(e.payload.type)"));
  assert.ok(read("../Workflows.tsx").includes("libraryReads(e.payload)"), "the library asks its model which read a fact moves");
  for (const type of WORKFLOW_FACTS) assert.deepEqual(libraryReads({ type }), { library: true, catalog: true }, `the library reloads on ${type}`);
  const events = read("../../../../crates/bisa-engine/src/events.rs");
  for (const variant of ["WorkflowChanged", "WorkflowArchived", "WorkflowDeleted"]) assert.ok(events.includes(`${variant} {`), `${variant} is a fact the engine says`);
});


test("the row the header reads falls behind the session once a save lands, and is read again — once", () => {
  // The row is the node's reading of the stored workflow: what it starts on, what turning On asks, whether it
  // runs in the workspace, its inputs as *Run…* asks them. A save moves the stored workflow, and the row with it.
  const row = (revision) => ({ workflow: { id: "01WF", revision }, problems: [], starts: [] });
  const session = (revision) => ({ base: { id: "01WF", revision } });
  assert.equal(rowBehind(row(3), session(3)), false, "as read");
  assert.equal(rowBehind(row(3), session(4)), true, "the designer's own save landed: a start moved onto a schedule shows its switch");
  assert.equal(rowBehind(row(4), session(4)), false, "read again: nothing more to ask");
  assert.equal(rowBehind(row(5), session(4)), false, "the row is ahead — another writer's save the session has yet to take");
  assert.equal(rowBehind(null, session(4)), false, "nothing read yet");
  assert.equal(rowBehind(row(3), null), false, "no session yet");
  assert.equal(rowBehind({ workflow: { id: "01OTHER", revision: 1 } }, session(4)), false, "another workflow's row is not this session's");
  const designer = readFileSync(new URL("../WorkflowDesigner.tsx", import.meta.url), "utf8");
  assert.ok(designer.includes("if (!rowBehind(data, session)) return;") && designer.includes("if (rowAsked.current === revision) return;"), "the designer reads its row again when it falls behind — once per revision, whatever is typed meanwhile");
});

test("a refused save is a conflict only when the stored copy moved past the revision it was sent at", () => {
  const moved = "the stored copy has moved: you edited revision 3, it is at 4";
  // Somebody saved first: the stored copy is ahead, and the person chooses.
  assert.deepEqual(refusalOutcome({ status: 409, message: moved, body: { error: moved } }, 3, stored(4, "Theirs")), { kind: "conflict", theirs: stored(4, "Theirs") });
  // A 409 over a copy nobody changed is the node refusing what the definition would do — in its own sentence.
  const held = "the public hook start `ticket` of `Triage` is still answered by a listening host: turn it off first";
  assert.deepEqual(refusalOutcome({ status: 409, message: held, body: { error: held } }, 3, stored(3)), { kind: "failed", message: held, problems: [] });
  // The stored copy could not be read: a failure, and the next attempt reads it again.
  assert.deepEqual(refusalOutcome({ status: 409, message: moved }, 3, null), { kind: "failed", message: moved, problems: [] });
  // A body the node could not read; a definition with problems; a node that answered nothing.
  assert.deepEqual(refusalOutcome({ status: 400, message: "unknown field `selected`", body: { error: "unknown field `selected`" } }, 3, null), { kind: "rejected", message: "unknown field `selected`" });
  const problems = [{ kind: "unreachable", step: "b", message: "nothing flows into b" }];
  assert.deepEqual(refusalOutcome({ status: 400, message: "1 problem", body: { error: "1 problem", problems } }, 3, null), { kind: "failed", message: "1 problem", problems });
  assert.deepEqual(refusalOutcome({ status: null, message: "waiting for the node…" }, 3, null), { kind: "failed", message: "waiting for the node…", problems: [] });
  assert.deepEqual(refusalOutcome({ status: 500, message: "internal error" }, 3, stored(9)), { kind: "failed", message: "internal error", problems: [] }, "only a 409 may be somebody else's save");
  assert.deepEqual([409, 400, 404, 500, 0, null].map(asksStored), [true, false, false, false, false, false]);
});

test("a save the node refuses over a copy nobody changed is said beside the draft and never offered as a choice that cannot be left", () => {
  const held = "the public hook start `ticket` is still answered by a listening host: turn it off first";
  let s = edit(open(stored(3)), { ...bodyOf(stored(3)), name: "Without its hook" }, 1000);
  s = saveStarted(s, saveRequest(s).body);
  s = settle(s, refusalOutcome({ status: 409, message: held, body: { error: held } }, 3, stored(3)));
  assert.equal(s.status, "failed");
  assert.equal(s.conflict, null, "no banner: nobody saved first");
  assert.equal(statusLine(s), `Not saved: ${held}`);
  assert.equal(dirty(s), true, "the draft stays where it was");
  assert.equal(s.base.revision, 3);
  // It is tried again at the backoff's pace, and stored once the host was turned off.
  assert.equal(saveDue(s, 5000, 800), 1000);
  const again = saveRequest(s);
  assert.deepEqual([again.revision, again.body.name], [3, "Without its hook"]);
  s = settle(saveStarted(s, again.body), { kind: "stored", workflow: stored(4, "Without its hook"), problems: [] });
  assert.deepEqual([s.status, s.base.revision, dirty(s)], ["idle", 4, false]);
  // Read the old way — any 409 a conflict — *Keep mine* would send the same body at the same revision, to the same refusal.
  const looped = keepMine(conflict(edit(open(stored(3)), { ...bodyOf(stored(3)), name: "Without its hook" }, 1000), stored(3)));
  assert.deepEqual([saveRequest(looped).revision, saveRequest(looped).body.name], [3, "Without its hook"]);
});

test("the header's line is the catalog's in every state, and the driver decides nothing of its own", () => {
  const clean = open(stored(3));
  assert.equal(statusLine(clean), "Saved");
  const typing = edit(clean, { ...bodyOf(stored(3)), name: "Edited" }, 1000);
  assert.equal(statusLine(typing), "Unsaved edits");
  assert.equal(statusLine(saveStarted(typing, saveRequest(typing).body)), "Saving…");
  const model = readFileSync(new URL("./designerSession.mjs", import.meta.url), "utf8");
  assert.ok(!model.includes('"saving…"') && !model.includes('"saved"'), "no word is spelt in the model");
  const driver = readFileSync(new URL("./useAutosave.ts", import.meta.url), "utf8");
  assert.ok(driver.includes("refusalOutcome(refusal, req.revision, theirs)") && driver.includes("asksStored(refusal.status)"));
  assert.ok(!driver.includes("=== 409") && !driver.includes("=== 400"), "which answer is which is the model's");
  assert.ok(!/catch \{\s*theirs = null;/.test(driver) && driver.includes('log.warn("designer", "the stored workflow could not be read after a save was refused"'), "a stored copy that could not be read is said in the log");
});

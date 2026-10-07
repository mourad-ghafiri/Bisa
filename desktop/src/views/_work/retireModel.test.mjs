/**
 * Retiring a goal or a workflow, as the dialog decides it. Run with
 * `node --test desktop/src/views/_work/retireModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { confirmWords, defaultChoices, deleteAvailable, destroys, historyLine, planOf, retireSections, retiredWords, runLine, runsLine, stopsLine, terminationOf, titleWords, touchedWorkstreams } from "./retireModel.mjs";

const project = (name, over = {}) => ({ id: name, slug: name, name, adopted: false, workstreams: 1, workstream_ids: [`ws-${name}`], sessions: 0, archived: false, ...over });
const preview = (over = {}) => ({ agents: 0, harnesses: 0, run: null, runs: [], history: 0, refusal: null, designs: 0, used_by: [], projects_born: [], projects_attached: [], ...over });
const NONE = { harnesses: 0, shells: 0, agents: 0 };

test("a used workflow can only be archived, and the door's fate is kept to what is available", () => {
  const used = preview({ used_by: [{ kind: "goal", id: "g1", label: "Ship it", live: false }] });
  assert.equal(deleteAvailable(used), false);
  assert.equal(deleteAvailable(preview()), true);
  const refused = preview({ refusal: "goal g cannot be deleted: its design d is still used by goal g2" });
  assert.equal(deleteAvailable(refused), false, "the node's refusal is read before anything stops");
  assert.deepEqual(defaultChoices("delete", refused), { thing: "archive", projects: "keep", tree: false });
  assert.deepEqual(defaultChoices("delete", used), { thing: "archive", projects: "keep", tree: false });
  assert.deepEqual(defaultChoices("delete", preview()), { thing: "delete", projects: "keep", tree: false });
  assert.deepEqual(defaultChoices("archive", preview()), { thing: "archive", projects: "keep", tree: false });
});

test("the sections are the facts in order, each worded for the choice", () => {
  const p = preview({
    agents: 2,
    run: { id: "r1", status: "running", live_steps: 2 },
    designs: 1,
    projects_born: [project("web"), project("infra", { adopted: true, sessions: 1 })],
    projects_attached: [project("shared")],
  });
  const archiveAll = retireSections("goal", p, { thing: "archive", projects: "archive", tree: false }, { harnesses: 1, shells: 1, agents: 2 });
  assert.deepEqual(
    archiveAll.map((s) => s.id),
    ["run", "stops", "designs", "born", "attached"],
  );
  assert.equal(archiveAll[0].lines[0], "Its run is cancelled: 2 steps are live.");
  assert.equal(archiveAll[1].lines[0], "1 harness terminated, 1 shell closed and 2 agent sessions aborted.");
  archiveAll.splice(0, 1);
  assert.equal(archiveAll[1].lines[0], "1 design drawn for it stay under it.");
  assert.match(archiveAll[2].lines[1], /infra — 1 workstream, 1 session, an adopted folder/);
  assert.match(archiveAll[2].lines[2], /put away/);
  assert.match(archiveAll[3].lines[0], /will not be deleted: shared stay as they are/);

  const deleteAll = retireSections("goal", p, { thing: "delete", projects: "delete", tree: true }, { harnesses: 0, shells: 0, agents: 2 });
  deleteAll.splice(0, 1);
  assert.equal(deleteAll[1].lines[0], "1 design drawn for it go with it.");
  assert.equal(deleteAll[1].tone, "danger");
  assert.match(deleteAll[2].lines[2], /Trash/);
  assert.match(deleteAll[2].lines[3], /infra: an adopted folder is never moved/);
  assert.match(deleteAll[3].lines[0], /stay in your workspace, detached/);

  const keep = retireSections("goal", p, { thing: "delete", projects: "keep", tree: false }, { harnesses: 0, shells: 0, agents: 2 });
  keep.splice(0, 1);
  assert.match(keep[2].lines[2], /stay in the workspace, detached/);
  assert.equal(retireSections("goal", preview(), { thing: "archive", projects: "keep", tree: false }, NONE).length, 0, "nothing to say about nothing");

  const refused = retireSections("goal", preview({ refusal: "goal g cannot be deleted: its design d is still used by goal g2" }), { thing: "archive", projects: "keep", tree: false }, NONE);
  assert.equal(refused[0].id, "refused");
  assert.match(refused[0].lines[0], /still used by goal g2\. It can be archived\./);

  const wf = retireSections(
    "workflow",
    preview({ used_by: [{ kind: "workflow", id: "w2", label: "release", live: false }, { kind: "goal", id: "g", label: "Ship it", live: true }], projects_born: [project("out")] }),
    { thing: "archive", projects: "keep", tree: false },
    NONE,
  );
  assert.deepEqual(
    wf.map((s) => s.id),
    ["used", "born"],
  );
  assert.match(wf[0].lines[0], /workflow release, goal Ship it \(running\) — while anything uses it/);
  assert.match(wf[0].lines[0], /stopped by retiring the goal, never the workflow/);
  assert.deepEqual(wf[0].holders.map((h) => [h.id, h.live]), [["w2", false], ["g", true]], "each holder is a door");
  assert.equal(wf[1].title, "The projects its steps made");
});

test("the run line says how many steps the cancel ends, and nothing when no run is going", () => {
  assert.equal(runLine(preview()), null);
  assert.equal(runLine(preview({ run: { id: "r", status: "running", live_steps: 1 } })), "Its run is cancelled: 1 step is live.");
  assert.equal(runLine(preview({ run: { id: "r", status: "pending", live_steps: 0 } })), "Its run is cancelled before another step starts.");
  assert.equal(stopsLine(NONE), null);
  assert.equal(stopsLine({ harnesses: 0, shells: 0, agents: 1 }), "1 agent session aborted.");
});

test("a workflow's runs of the workspace that are going are cancelled, whatever its fate, and its history kept or gone with it", () => {
  const going = preview({
    runs: [
      { id: "r1", status: "running", live_steps: 2 },
      { id: "r2", status: "waiting", live_steps: 1 },
    ],
    history: 5,
  });
  assert.equal(runsLine(preview()), null);
  assert.equal(runsLine(going), "Its 2 runs in the workspace are cancelled: 3 steps are live.");
  assert.equal(runsLine(preview({ runs: [{ id: "r", status: "running", live_steps: 0 }] })), "Its run in the workspace is cancelled before another step starts.");
  const archive = { thing: "archive", projects: "keep", tree: false };
  const del = { thing: "delete", projects: "keep", tree: false };
  assert.equal(historyLine(preview(), archive), null);
  assert.equal(historyLine(going, archive), "Its 5 runs in the workspace stay as its history.");
  assert.equal(historyLine(preview({ history: 1 }), del), "Its run in the workspace goes with it.");

  const archived = retireSections("workflow", going, archive, NONE);
  assert.deepEqual(archived.map((s) => [s.id, s.tone]), [["run", "warn"], ["history", "quiet"]]);
  assert.deepEqual(archived[0].lines, ["Its 2 runs in the workspace are cancelled: 3 steps are live."]);
  const deleted = retireSections("workflow", going, del, NONE);
  assert.deepEqual(deleted.map((s) => [s.id, s.tone]), [["run", "warn"], ["history", "danger"]]);
  assert.equal(deleted[1].lines[0], "Its 5 runs in the workspace go with it.");
  assert.deepEqual(retireSections("goal", preview({ history: 3 }), archive, NONE), [], "a goal has no runs of the workspace");
});

test("what the plan terminates is counted once from the preview and this app's own stores, in the touched workstreams only", () => {
  const p = preview({
    agents: 1,
    harnesses: 1,
    projects_born: [project("web", { workstream_ids: ["w1", "w2"] })],
  });
  const live = { status: "live" };
  const terminals = [
    { key: "t1", scope: "goal", id: "g", harness: "claude-code", liveness: live },
    { key: "t2", scope: "workstream", id: "w1", harness: null, liveness: live },
    { key: "t3", scope: "workstream", id: "w1", harness: "codex", liveness: live },
    { key: "t4", scope: "workstream", id: "w9", harness: null, liveness: live },
    { key: "t5", scope: "workstream", id: "w2", harness: null, liveness: { status: "exited", code: 0 } },
  ];
  const sessions = [
    { id: "s1", kind: "worker", state: { state: "running" }, goal: "g", workstream: "w1" },
    { id: "s2", kind: "worker", state: { state: "running" }, goal: "other", workstream: "w2" },
    { id: "s3", kind: "worker", state: { state: "done" }, goal: "other", workstream: "w2" },
    { id: "s5", kind: "worker", state: { state: "aborted" }, goal: "other", workstream: "w1" },
    { id: "s6", kind: "worker", state: { state: "failed", reason: "x" }, goal: "other", workstream: "w2" },
    { id: "s4", kind: "terminal", state: { state: "running" }, goal: null, workstream: "w1" },
  ];
  assert.deepEqual(touchedWorkstreams(p, { thing: "delete", projects: "keep", tree: false }), new Set(), "kept projects are not touched");
  assert.deepEqual(touchedWorkstreams(p, { thing: "delete", projects: "archive", tree: false }), new Set(["w1", "w2"]));

  const kept = terminationOf(p, sessions, terminals, { thing: "delete", projects: "keep", tree: false }, "g");
  assert.deepEqual(kept, { harnesses: 1, shells: 0, agents: 1 }, "only the goal's own: its harness tab, its engine session");

  const touched = terminationOf(p, sessions, terminals, { thing: "delete", projects: "delete", tree: false }, "g");
  assert.deepEqual(
    touched,
    { harnesses: 2, shells: 1, agents: 2 },
    "the goal's tab and the codex tab; the one live shell in w1; the goal's session and the other goal's session in w2 — never a done, aborted or failed one, never the interactive row, never w9",
  );

  const wf = terminationOf(p, sessions, terminals, { thing: "archive", projects: "archive", tree: false }, null);
  assert.deepEqual(wf, { harnesses: 1, shells: 1, agents: 2 }, "a workflow has no goal scope of its own; every engine session in the touched workstreams counts");
  const wfMore = terminationOf(preview({ agents: 5, projects_born: [project("web", { workstream_ids: ["w1", "w2"] })] }), sessions, terminals, { thing: "archive", projects: "archive", tree: false }, null);
  assert.equal(wfMore.agents, 5, "sessions the node counts that this roster has not heard of yet are still counted — once");
  const goalMore = terminationOf(preview({ agents: 5, projects_born: [project("web", { workstream_ids: ["w1", "w2"] })] }), sessions, terminals, { thing: "delete", projects: "delete", tree: false }, "g");
  assert.equal(goalMore.agents, 6, "on a goal the preview's own and the other goal's session in w2 are disjoint, so they add");
  assert.deepEqual(terminationOf(preview(), null, null, { thing: "delete", projects: "delete", tree: false }, "g"), NONE, "empty stores count nothing");
});

test("the button reads the plan back, and is dangerous only when something is deleted", () => {
  const none = preview();
  const two = preview({ projects_born: [project("a"), project("b")] });
  assert.equal(confirmWords("goal", { thing: "archive", projects: "keep", tree: false }, none), "Archive goal");
  assert.equal(confirmWords("goal", { thing: "archive", projects: "keep", tree: false }, two), "Archive goal, keep 2 projects");
  assert.equal(confirmWords("goal", { thing: "archive", projects: "archive", tree: false }, two), "Archive goal and 2 projects");
  assert.equal(confirmWords("goal", { thing: "delete", projects: "archive", tree: false }, two), "Delete goal, archive 2 projects");
  assert.equal(confirmWords("workflow", { thing: "delete", projects: "delete", tree: true }, preview({ projects_born: [project("a")] })), "Delete workflow and 1 project");
  assert.equal(destroys({ thing: "archive", projects: "archive", tree: false }), false);
  assert.equal(destroys({ thing: "archive", projects: "delete", tree: false }), true);
  assert.equal(destroys({ thing: "delete", projects: "keep", tree: false }), true);
});

test("the plan is the node's shape, and the Trash only means something under delete", () => {
  assert.deepEqual(planOf("goal", { thing: "archive", projects: "keep", tree: true }), { goal: "archive", projects: "keep", tree: false });
  assert.deepEqual(planOf("goal", { thing: "delete", projects: "delete", tree: true }), { goal: "delete", projects: "delete", tree: true });
  assert.deepEqual(planOf("workflow", { thing: "archive", projects: "archive", tree: true }), { workflow: "archive", projects: "archive", tree: false });
});

test("the title names the thing under the fate chosen — the door's at first, the person's once they changed their mind inside", () => {
  assert.equal(titleWords("archive", "Ship the report"), "Archive Ship the report?");
  assert.equal(titleWords("delete", "Ship the report"), "Delete Ship the report?");
  // Opened from *Delete…*, then *Archive* chosen inside: the title follows the choice, as the button does.
  const door = defaultChoices("delete", preview());
  assert.equal(titleWords(door.thing, "Weekly digest"), "Delete Weekly digest?");
  assert.equal(titleWords({ ...door, thing: "archive" }.thing, "Weekly digest"), "Archive Weekly digest?");
  assert.equal(confirmWords("workflow", { ...door, thing: "archive", projects: "keep" }, preview()), "Archive workflow", "the title and the button never disagree");
  // A workflow that was used cannot be deleted: the door's fate is kept to what is available, and the title with it.
  const used = preview({ used_by: [{ kind: "goal", id: "g1", label: "Ship it", live: false }] });
  assert.equal(titleWords(defaultChoices("delete", used).thing, "Weekly digest"), "Archive Weekly digest?");
  const dialog = readFileSync(new URL("./RetireDialog.tsx", import.meta.url), "utf8");
  assert.ok(dialog.includes("title={titleWords(choices.thing, name)}"), "the dialog reads the choice, never the door it was opened from");
});

test("once it happened, one sentence says the thing, its fate and what stopped with it — on the goal's page, in the designer and on the card", () => {
  const some = { harnesses: 1, shells: 0, agents: 2 };
  assert.equal(retiredWords("goal", "archive", NONE), "Goal archived.");
  assert.equal(retiredWords("goal", "archive", some), "Goal archived — 1 harness terminated and 2 agent sessions aborted.");
  assert.equal(retiredWords("goal", "delete", NONE), "Goal deleted.");
  assert.equal(retiredWords("goal", "delete", some), "Goal deleted — 1 harness terminated and 2 agent sessions aborted.");
  assert.equal(retiredWords("workflow", "archive", null), "Workflow archived.");
  assert.equal(retiredWords("workflow", "archive", some), "Workflow archived — 1 harness terminated and 2 agent sessions aborted.");
  assert.equal(retiredWords("workflow", "delete", NONE), "Workflow deleted.");
  assert.equal(retiredWords("workflow", "delete", some), "Workflow deleted — 1 harness terminated and 2 agent sessions aborted.");
  // The node's own word rides after this app's: what had to be terminated, what could not be ended, the goals closed with it.
  const node = { sessions: 2, terminated: 1, still_live: 0, children: ["c1"] };
  assert.equal(retiredWords("goal", "delete", some, node), "Goal deleted — 1 harness terminated and 2 agent sessions aborted, and 1 harness did not answer and was terminated and 1 spawned goal closed too.");
  assert.equal(retiredWords("goal", "archive", NONE, node), "Goal archived — 1 harness did not answer and was terminated and 1 spawned goal closed too.");
  assert.equal(retiredWords("goal", "archive", NONE, { sessions: 3, terminated: 0, still_live: 0, children: [] }), "Goal archived.", "the sessions this app counted are not said twice");
  assert.equal(retiredWords("goal", "archive", NONE, null), "Goal archived.", "an older node says nothing more");
  for (const screen of ["../GoalDetail.tsx", "../WorkflowDesigner.tsx", "../_workflow/WorkflowCard.tsx"]) {
    const text = readFileSync(new URL(screen, import.meta.url), "utf8");
    assert.ok(text.includes("retiredWords(") && !text.includes("terminationWords("), `${screen} says the model's sentence`);
  }
});

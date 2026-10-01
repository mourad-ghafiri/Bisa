/**
 * Retiring a goal, as the dialog walks a person through it: the node's
 * preview, the choices, what stops, what the button says, and the plan that
 * goes on the wire — then the same for a workflow something still uses.
 * Run with `node --test desktop/src/scenarios/retire.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { confirmWords, defaultChoices, deleteAvailable, destroys, planOf, retireSections, runLine, stopsLine, terminationOf, touchedWorkstreams } from "../views/_work/retireModel.mjs";

const project = (name, over = {}) => ({ id: name, slug: name, name, adopted: false, workstreams: 1, workstream_ids: [`ws-${name}`], sessions: 0, archived: false, ...over });
const preview = (over = {}) => ({ agents: 0, harnesses: 0, run: null, runs: [], history: 0, refusal: null, designs: 0, used_by: [], projects_born: [], projects_attached: [], ...over });
const live = { status: "live" };

test("archiving a running goal: the run is cancelled, what stands in its places stops, its projects are put away, and the button reads the plan back", () => {
  const p = preview({
    agents: 1,
    harnesses: 1,
    run: { id: "r1", status: "running", live_steps: 2 },
    projects_born: [project("web"), project("infra", { adopted: true, sessions: 1 })],
    projects_attached: [project("shared")],
  });
  const terminals = [
    { key: "t1", scope: "goal", id: "g", harness: "claude-code", liveness: live },
    { key: "t2", scope: "workstream", id: "ws-web", harness: null, liveness: live },
    { key: "t3", scope: "workstream", id: "ws-web", harness: null, liveness: { status: "exited", code: 0 } },
  ];
  const roster = [
    { id: "s1", kind: "worker", state: { state: "running" }, goal: "g", workstream: "ws-web" },
    { id: "s2", kind: "worker", state: { state: "waiting", on: { on: "permission", tool: "Bash", gate_id: "x" } }, goal: "other", workstream: "ws-infra" },
  ];

  // The dialog opens on Archive with the projects kept.
  let choices = defaultChoices("archive", p);
  assert.deepEqual(choices, { thing: "archive", projects: "keep", tree: false });
  assert.equal(deleteAvailable(p), true);
  assert.deepEqual(touchedWorkstreams(p, choices), new Set(), "kept projects: nothing of theirs is touched");
  let counts = terminationOf(p, roster, terminals, choices, "g");
  assert.deepEqual(counts, { harnesses: 1, shells: 0, agents: 1 }, "only the goal's own stop");
  assert.equal(runLine(p), "Its run is cancelled: 2 steps are live.");
  assert.equal(stopsLine(counts), "1 harness terminated and 1 agent session aborted.");
  assert.equal(confirmWords("goal", choices, p), "Archive goal, keep 2 projects");
  assert.equal(destroys(choices), false);

  // The person chooses to archive the projects too: their workstreams are touched.
  choices = { ...choices, projects: "archive" };
  assert.deepEqual(touchedWorkstreams(p, choices), new Set(["ws-web", "ws-infra"]));
  counts = terminationOf(p, roster, terminals, choices, "g");
  assert.deepEqual(counts, { harnesses: 1, shells: 1, agents: 2 }, "the live shell in web and the other goal's waiting session in infra join; the exited shell does not");
  assert.equal(stopsLine(counts), "1 harness terminated, 1 shell closed and 2 agent sessions aborted.");
  const sections = retireSections("goal", p, choices, counts);
  assert.deepEqual(sections.map((s) => s.id), ["run", "stops", "born", "attached"]);
  assert.equal(sections[1].lines[0], stopsLine(counts));
  assert.match(sections[2].lines[2], /put away/);
  assert.match(sections[3].lines[0], /shared stay as they are/);
  assert.equal(confirmWords("goal", choices, p), "Archive goal and 2 projects");
  assert.deepEqual(planOf("goal", choices), { goal: "archive", projects: "archive", tree: false });
  assert.equal(destroys(choices), false, "archiving destroys nothing");

  // Delete with the tree: dangerous, and said so.
  choices = { thing: "delete", projects: "delete", tree: true };
  assert.equal(destroys(choices), true);
  const gone = retireSections("goal", p, choices, counts);
  assert.match(gone.find((s) => s.id === "born").lines[3], /infra: an adopted folder is never moved/);
  assert.deepEqual(planOf("goal", choices), { goal: "delete", projects: "delete", tree: true });
});

test("a goal whose design is still used cannot be deleted: the refusal is read first, the door falls back to Archive, and the plan follows", () => {
  const p = preview({ refusal: "goal g cannot be deleted: its design d is still used by goal g2" });
  assert.equal(deleteAvailable(p), false);
  const choices = defaultChoices("delete", p);
  assert.deepEqual(choices, { thing: "archive", projects: "keep", tree: false }, "asked to delete, offered to archive");
  const sections = retireSections("goal", p, choices, { harnesses: 0, shells: 0, agents: 0 });
  assert.equal(sections[0].id, "refused");
  assert.match(sections[0].lines[0], /still used by goal g2\. It can be archived\./);
  assert.deepEqual(planOf("goal", choices), { goal: "archive", projects: "keep", tree: false });
});

test("a workflow something uses is archived, never deleted; its sessions in the touched workstreams are counted once; another workflow and a goal are named as holders", () => {
  const p = preview({ agents: 2, used_by: [{ kind: "workflow", id: "w2", label: "release", live: false }, { kind: "goal", id: "g", label: "Ship it", live: true }], projects_born: [project("out")] });
  assert.equal(deleteAvailable(p), false);
  const choices = defaultChoices("delete", p);
  assert.equal(choices.thing, "archive");
  const roster = [
    { id: "s1", kind: "worker", state: { state: "running" }, goal: "g", workstream: "ws-out" },
    { id: "s2", kind: "worker", state: { state: "thinking" }, goal: "g", workstream: "ws-out" },
    { id: "s3", kind: "worker", state: { state: "running" }, goal: "g", workstream: "ws-elsewhere" },
  ];
  const all = { ...choices, projects: "archive" };
  const counts = terminationOf(p, roster, [], all, null);
  assert.deepEqual(counts, { harnesses: 0, shells: 0, agents: 2 }, "the preview's two and the roster's two in ws-out are the same sessions");
  const sections = retireSections("workflow", p, all, counts);
  assert.deepEqual(sections.map((s) => s.id), ["stops", "used", "born"]);
  assert.deepEqual(sections[1].holders.map((h) => [h.kind, h.id, h.live]), [["workflow", "w2", false], ["goal", "g", true]]);
  assert.match(sections[1].lines[0], /stopped by retiring the goal, never the workflow/);
  assert.deepEqual(planOf("workflow", all), { workflow: "archive", projects: "archive", tree: false });
  assert.equal(confirmWords("workflow", all, p), "Archive workflow and 1 project");
});

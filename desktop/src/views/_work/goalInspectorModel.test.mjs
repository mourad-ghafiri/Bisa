/**
 * The goal's Details pane, as facts: the panel the address names, the run's
 * card, the spend, the projects, where the Workflow Agent stands, where the
 * goal came from. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_work/goalInspectorModel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";

import { PANELS, PROJECTS_NAMED, designLine, originChips, panelOf, projectsWords, runCardWords, spendWords, workstreamWord } from "./goalInspectorModel.mjs";

const run = (over = {}) => ({ id: "01J0000000000000000000RUN2", workflow: { name: "Bug fix", revision: 4 }, outcome: null, cancelled: null, ...over });
const summaries = [
  { id: "01J0000000000000000000RUN1", queued_at: 10 },
  { id: "01J0000000000000000000RUN2", queued_at: 20 },
  { id: "01J0000000000000000000RUN3", queued_at: 30 },
];

test("the address names a panel, and anything else is Details", () => {
  assert.deepEqual([...PANELS], ["details", "work", "projects", "files"]);
  for (const p of PANELS) assert.equal(panelOf(p), p);
  for (const raw of [undefined, null, "", "Details", "inspector", 3]) assert.equal(panelOf(raw), "details", `${JSON.stringify(raw)}`);
});

test("the run's card says which run of how many, the revision it froze, and how it ended once it has", () => {
  assert.equal(runCardWords(run(), summaries), "run 2 of 3 · rev 4");
  assert.equal(runCardWords(run({ outcome: "done" }), summaries), "run 2 of 3 · rev 4 · done");
  assert.equal(runCardWords(run({ outcome: "failed" }), summaries), "run 2 of 3 · rev 4 · failed");
  assert.equal(runCardWords(run({ cancelled: { cause: "stopped" } }), summaries), "run 2 of 3 · rev 4 · stopped", "a cancel says its cause");
  assert.equal(runCardWords(run({ outcome: "done", cancelled: { cause: "closed", reason: { reason: "superseded", by: "G2" } } }), summaries), "run 2 of 3 · rev 4 · closed", "a cancel outranks an outcome");
  assert.equal(runCardWords(run({ id: "nobody" }), summaries), "run 3 of 3 · rev 4", "a run the list does not hold yet is its newest");
  assert.equal(runCardWords(run(), null), "run 0 of 0 · rev 4", "a list nobody read never throws");
});

test("what the goal has spent is one line: tokens, cost, seconds", () => {
  assert.equal(spendWords({ tokens: 1234567, usd_cents: 1250, wall_clock_secs: 95 }), "1,234,567 tokens · $12.50 · 95 s");
  assert.equal(spendWords({ tokens: 0, usd_cents: 40, wall_clock_secs: 0 }), "0 tokens · 40¢ · 0 s");
  assert.equal(spendWords(null), "0 tokens · 0¢ · 0 s", "nothing read yet is nothing spent");
});

test("the projects card names the first few and counts the rest, then counts projects and workstreams", () => {
  assert.equal(projectsWords([]), null);
  assert.equal(projectsWords(null), null);
  const rows = (n) => Array.from({ length: n }, (_, i) => ({ project: { name: `p${i + 1}` }, workstreams: i }));
  assert.deepEqual(projectsWords(rows(1)), { names: "p1", counts: "1 project · 0 workstreams" });
  assert.deepEqual(projectsWords(rows(2)), { names: "p1 · p2", counts: "2 projects · 1 workstream" });
  assert.deepEqual(projectsWords(rows(PROJECTS_NAMED)), { names: "p1 · p2 · p3", counts: "3 projects · 3 workstreams" });
  assert.deepEqual(projectsWords(rows(5)), { names: "p1 · p2 · p3 +2", counts: "5 projects · 10 workstreams" });
  assert.equal(projectsWords([{ project: { name: "p" } }]).counts, "1 project · 0 workstreams", "a row without a count counts none");
});

test("the Workflow Agent's standing is said while it designs or repairs, and when designing is off — nothing between", () => {
  assert.equal(designLine("auto", { design_enabled: true, phase: "design" }), "Workflow Agent · designing the workflow");
  assert.equal(designLine("guided", { design_enabled: true, phase: "repair" }), "Workflow Agent · repairing after a failed step");
  assert.equal(designLine("guided", { design_enabled: true, phase: null }), null);
  assert.equal(designLine("guided", { design_enabled: true, phase: "invented" }), null, "a phase nobody knows is no sentence built in code");
  assert.equal(designLine("auto", { design_enabled: false, phase: "design" }), "This goal is auto, but designing is off on this node — nobody is designing it.");
  assert.equal(designLine("guided", null), "This goal is guided, but designing is off on this node — nobody is designing it.");
});

test("a goal's origin: its mode, the goal it refines or the run whose step made it — each a door — and the run it is on", () => {
  const captured = originChips({ origin: { origin: "captured" } }, null, "auto");
  assert.deepEqual(captured, [{ id: "mode", label: "Auto", icon: "mode" }]);
  const spawned = originChips({ origin: { origin: "spawned", parent: "01PARENT" } }, run(), "guided");
  assert.deepEqual(spawned.map((c) => [c.id, c.label, c.route ?? null]), [
    ["mode", "Guided", null],
    ["parent", "refines a goal", { name: "goal", id: "01PARENT" }],
    ["run", "run 00RUN2 · Bug fix", null],
  ]);
  // A goal a `spawn` step of a run in the workspace captured refines nothing: its origin names the run and the step.
  const born = originChips({ origin: { origin: "run", run: "01RUN", step: "escalate" } }, null, "auto");
  assert.deepEqual(born.map((c) => [c.id, c.label, c.route ?? null]), [
    ["mode", "Auto", null],
    ["born", "made by a run · step escalate", { name: "run", id: "01RUN" }],
  ]);
  assert.match(born[1].title, /may have gone since/, "an origin is history: the door may lead to a thing that is gone");
  assert.deepEqual(originChips({}, null, "manual").map((c) => c.label), ["Manual"], "a goal with no origin read still has its mode");
  assert.deepEqual(originChips({ origin: { origin: "spawned" } }, null, "auto").map((c) => c.id), ["mode"], "no parent named: no door to nowhere");
});

test("a checkout's word is the name it was given, else its branch, else that it is a copy", () => {
  assert.equal(workstreamWord({ id: "01J00000000000000000ABCDEF", name: "login fix", kind: { kind: "worktree", branch: "fix/login" } }), "login fix");
  assert.equal(workstreamWord({ id: "01J00000000000000000ABCDEF", name: null, kind: { kind: "worktree", branch: "fix/login" } }), "fix/login");
  assert.equal(workstreamWord({ id: "01J00000000000000000ABCDEF", kind: { kind: "copy" } }), "copy · ABCDEF");
});

test("the pane draws what the model says and spells no sentence of its own", () => {
  const pane = readFileSync(new URL("./GoalInspector.tsx", import.meta.url), "utf8");
  for (const call of ["panelOf(param)", "runCardWords(run, runs)", "spendWords(spent)", "projectsWords(data?.projects)", "designLine(mode, guidance)", "originChips(goal, run, mode)", "workstreamWord(w)"]) assert.ok(pane.includes(call), call);
  for (const spelt of ["None yet", "} tokens", "workstreams\n", "The Workflow Agent names", "run {", "copy ·", '"1 project"']) assert.ok(!pane.includes(spelt), `the pane spells ${JSON.stringify(spelt)}`);
  assert.ok(!pane.includes("work-goal-inspector-add-folder-work"), "nothing is created or attached here: no door says otherwise");
  assert.ok(/log\.warn\("goal", "a project's workstreams could not be read/.test(pane), "a read that failed is said in the log, never swallowed");
});

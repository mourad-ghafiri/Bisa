/**
 * What a project's origin says. Run with `node --test desktop/src/views/_work/projectOriginModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { bornWords, madeByAgentStep, madeByGoal, madeByStepWords, projectsMadeByStep, projectsMadeByWorkflow, stepOf } from "./projectOriginModel.mjs";

const byHand = { origin: "workspace" };
const fromGoal = { origin: "goal", goal: "g1" };
const byDesign = { origin: "goal", goal: "g1", step: { run: "r1", step: "build", workflow: "d1" } };
const byLibrary = { origin: "step", goal: "g1", run: "r2", step: "implement", workflow: "w1" };
const row = (id, origin) => ({ project: { id, origin }, path: `/ws/${id}`, exists: true, goals: [] });

test("the step that made a project is read from either variant — flat on step, nested on goal", () => {
  assert.equal(stepOf(byHand), null);
  assert.equal(stepOf(fromGoal), null);
  assert.deepEqual(stepOf(byDesign), { run: "r1", step: "build", workflow: "d1" });
  assert.deepEqual(stepOf(byLibrary), { run: "r2", step: "implement", workflow: "w1" });
  assert.equal(stepOf(null), null);
  assert.ok(!madeByAgentStep(fromGoal) && madeByAgentStep(byDesign) && madeByAgentStep(byLibrary));
});

test("born of a goal means the goal variant with that goal, whoever pressed the button", () => {
  assert.ok(madeByGoal(fromGoal, "g1"));
  assert.ok(madeByGoal(byDesign, "g1"), "a design's step makes the goal's project");
  assert.ok(!madeByGoal(byLibrary, "g1"), "a library step's project is the workflow's, though it runs on the goal");
  assert.ok(!madeByGoal(fromGoal, "g2"));
});

test("the words say which door it came through", () => {
  assert.equal(bornWords(byHand), "");
  assert.equal(bornWords(fromGoal), " from its goal");
  assert.equal(bornWords(byDesign), " by a step of its goal's design");
  assert.equal(bornWords(byLibrary), " by a workflow step");
});

test("a library step's project names its goal, and a run in the workspace has none to name", () => {
  const title = (id) => `Goal ${id}`;
  assert.equal(madeByStepWords(byLibrary, title), "Created by step implement of a run on Goal g1");
  const inTheWorkspace = { origin: "step", run: "r7", step: "build", workflow: "w1" };
  assert.equal(madeByStepWords(inTheWorkspace, title), "Created by step build of a run in the workspace");
  assert.equal(madeByStepWords({ ...inTheWorkspace, goal: null }, title), "Created by step build of a run in the workspace", "the wire's null is no goal too");
});

test("a run's step and a library workflow each find the projects they made", () => {
  const inTheWorkspace = { origin: "step", run: "r7", step: "build", workflow: "w1" };
  const rows = [
    row("p1", byHand),
    row("p2", fromGoal),
    row("p3", byDesign),
    row("p4", byLibrary),
    row("p5", { origin: "step", goal: "g2", run: "r9", step: "build", workflow: "w1" }),
    row("p6", inTheWorkspace),
  ];
  assert.deepEqual(projectsMadeByStep(rows, "r1", "build").map((r) => r.project.id), ["p3"], "the design's step, on its run");
  assert.deepEqual(projectsMadeByStep(rows, "r2", "implement").map((r) => r.project.id), ["p4"], "a library step, on its run");
  assert.deepEqual(projectsMadeByStep(rows, "r9", "build").map((r) => r.project.id), ["p5"], "the same step id in another run is another project");
  assert.deepEqual(projectsMadeByStep(rows, "r7", "build").map((r) => r.project.id), ["p6"], "a run of the workspace's step: no goal at all");
  assert.deepEqual(projectsMadeByStep(rows, "r1", "implement"), [], "a step of another run made nothing here");
  assert.deepEqual(projectsMadeByWorkflow(rows, "w1").map((r) => r.project.id), ["p4", "p5", "p6"], "a library workflow's projects across goals and the workspace");
  assert.deepEqual(projectsMadeByWorkflow(rows, "d1"), [], "a design is not a library workflow: its projects are its goal's");
});

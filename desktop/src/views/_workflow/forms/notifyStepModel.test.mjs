import test from "node:test";
import assert from "node:assert/strict";
import { scopeListed, scopeOptions } from "./notifyStepModel.mjs";

test("a notify scope is listed when it is none, a channel the workspace has, a goal or a workstream handed, or a declared text input", () => {
  const channels = ["general", "ops"];
  const inputs = ["channel", "desk"];
  for (const listed of [null, undefined, "", "general", "ops", "{inputs.channel}", "{inputs.desk}"]) assert.equal(scopeListed(listed, channels, inputs), true, String(listed));
  for (const unlisted of ["01ARZ3NDEKTSV4RRFFQ69G5FAV", "{inputs.gone}", "{steps.a.output.where}", "Ops"]) assert.equal(scopeListed(unlisted, channels, inputs), false, unlisted);
  assert.equal(scopeListed("general", [], []), false, "no channels listed: shown as it is");
  assert.equal(scopeListed("x", null, null), false);
  const goal = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
  const ws = "01BX5ZZKBKACTAV9WEVGEMMVRZ";
  assert.equal(scopeListed(goal, channels, inputs, [goal], []), true, "a goal handed is listed");
  assert.equal(scopeListed(ws, channels, inputs, [], [ws]), true, "a workstream handed is listed");
  assert.equal(scopeListed(goal, channels, inputs, [], [ws]), false, "a goal not handed is shown as it is");
});

test("the scope options read in words: the run's own conversation first, channels, open goals by title or statement cut short, workstreams by their project and name or tail, inputs read at start", () => {
  const long = "x".repeat(60);
  const o = scopeOptions({
    channels: [{ id: "general", name: "general" }],
    goals: [
      { id: "g1", title: "Ship it", statement: "ship the release" },
      { id: "g2", title: null, statement: long },
    ],
    workstreams: [
      { project_name: "shop", workstream: { id: "01BX5ZZKBKACTAV9WEVGEMMVRZ", name: "feature/login" } },
      { project_name: null, workstream: { id: "01BX5ZZKBKACTAV9WEVGEMMVRZ", name: null } },
    ],
    textInputs: [{ name: "desk" }],
  });
  assert.equal(o.first, "The goal's conversation");
  assert.deepEqual(o.channels, [{ value: "general", label: "channel · general" }]);
  assert.equal(o.goals[0].label, "goal · Ship it");
  assert.equal(o.goals[1].value, "g2");
  assert.equal(o.goals[1].label.length, "goal · ".length + 48, "a statement is cut short");
  assert.ok(o.goals[1].label.endsWith("…"));
  assert.deepEqual(o.workstreams[0], { value: "01BX5ZZKBKACTAV9WEVGEMMVRZ", label: "shop · feature/login" });
  assert.equal(o.workstreams[1].label, " · EMMVRZ", "a nameless one by its id's tail");
  assert.deepEqual(o.inputs, [{ value: "{inputs.desk}", label: "input · desk" }]);
  assert.deepEqual(scopeOptions({ channels: null, goals: null, workstreams: null, textInputs: null }).goals, []);
});

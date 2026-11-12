import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { heldFor, holding } from "./heldAnswerModel.mjs";

test("a write's answer stands until the list it changed is read again", () => {
  const listed = ["survey"];
  // Attached twice, quickly: the second change is built from the first one's answer.
  const first = holding(listed, ["survey", "brevity"]);
  assert.deepEqual(heldFor(listed, first), ["survey", "brevity"]);
  const second = [...heldFor(listed, first), "house-style"];
  assert.deepEqual(second, ["survey", "brevity", "house-style"], "the first attach is still there");
  // Built from the list a read gave before the write, it would have dropped it.
  assert.deepEqual([...listed, "house-style"], ["survey", "house-style"]);
  // The read lands — another list, even an equal one: the read is the truth again.
  assert.equal(heldFor(["survey", "brevity"], first), null);
  assert.equal(heldFor(listed, null), null);
  assert.equal(heldFor(listed, undefined), null);
});

test("every whole-list control builds its next write from what the node last answered", () => {
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const agents = read("../Agents.tsx");
  assert.ok(agents.includes("const [held, hold] = useHeldAnswer<string[], string[]>(ids);"), "an agent's skills and servers");
  assert.ok(agents.includes("const shown = held ?? ids;"));
  assert.ok(agents.includes("onAttach={(id) => run([...shown, id])}") && agents.includes("run(shown.filter((v) => v !== id))"));
  const goal = read("./GoalInspector.tsx");
  assert.ok(goal.includes("useHeldAnswer<Assignee[] | undefined, string[]>(view.goal.assignees)"), "a goal's assignees");
  const teams = read("../Teams.tsx");
  assert.ok(teams.includes("useHeldAnswer<TeamDef, TeamDef>(team)"), "a team's roster");
});

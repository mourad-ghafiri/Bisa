import test from "node:test";
import assert from "node:assert/strict";
import { addressee } from "./agentRailModel.mjs";

test("the addressee is the project's default agent, and the general agent when that one cannot take it", () => {
  const general = { id: "general-agent", name: "General Agent", enabled: true };
  const workflow = { id: "workflow-agent", name: "Workflow Agent", enabled: true };
  const reviewer = { id: "reviewer", name: "Reviewer", enabled: true };
  const asleep = { id: "asleep", name: "Asleep", enabled: false };
  const agents = [general, workflow, reviewer, asleep];
  assert.equal(addressee(agents, "reviewer", "general-agent"), reviewer);
  assert.equal(addressee(agents, null, "general-agent"), general, "unset: the general agent");
  assert.equal(addressee(agents, "nope", "general-agent"), general, "unknown: the general agent");
  assert.equal(addressee(agents, "asleep", "general-agent"), general, "disabled: the general agent");
  assert.equal(addressee(agents, "workflow-agent", "general-agent"), general, "the Workflow Agent is never a checkout's default");
  assert.equal(addressee(agents, "workflow-agent", "general-agent", "project"), general, "nor a project's");
  assert.equal(addressee(agents, "workflow-agent", "general-agent", "goal"), workflow, "a conversation about a goal may name it");
  assert.equal(addressee(agents, "workflow-agent", "general-agent", "workflow"), workflow);
  assert.equal(addressee([], "reviewer", "general-agent"), null, "nobody at all");
});

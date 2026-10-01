/**
 * What a picker row says, and what a roster keeps.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import { candidateOf, resolveRoster, roleLineOf, warningFor } from "./participantsModel.mjs";

const agent = (over = {}) => ({
  id: "developer",
  name: "Developer",
  description: null,
  system_prompt: "You write the code.\nYou test it.",
  harness: "claude-code",
  tags: ["engineering"],
  pubkey: "a".repeat(64),
  photo: null,
  enabled: true,
  ...over,
});

test("a roster keeps an id whose agent is gone, marked by the id itself, in the roster's order", () => {
  const entries = resolveRoster(["reviewer", "developer"], [agent()]);
  assert.deepEqual(
    entries.map((e) => [e.id, e.name, e.agent?.id ?? null]),
    [
      ["reviewer", "reviewer", null],
      ["developer", "Developer", "developer"],
    ],
  );
});

test("a disabled agent is marked, an agent whose harness this node lacks is marked, and an unknown roster marks nothing", () => {
  assert.equal(warningFor(agent({ enabled: false }), new Set(["claude-code"])), "disabled");
  assert.equal(warningFor(agent(), new Set(["codex"])), "claude-code not installed");
  assert.equal(warningFor(agent(), null), null, "`null` is not known yet, not missing");
  assert.equal(warningFor(agent(), new Set(["claude-code"])), null);
});

test("the role line is the description, else the prompt's first line, else nothing — never an empty line", () => {
  assert.equal(roleLineOf(agent({ description: "Ships features" })), "Ships features");
  assert.equal(roleLineOf(agent()), "You write the code.");
  assert.equal(roleLineOf(agent({ system_prompt: "" })), null);
  assert.equal(roleLineOf(agent({ description: "" })), "You write the code.", "an empty description is no description");
});

test("a picker row carries the caller's id, the agent's face and the warning", () => {
  const row = candidateOf(agent({ enabled: false }), "pk-1", new Set(["claude-code"]));
  assert.deepEqual(row, {
    id: "pk-1",
    name: "Developer",
    kind: "agent",
    description: "You write the code.",
    harness: "claude-code",
    tags: ["engineering"],
    avatar: "a".repeat(64),
    photo: null,
    warning: "disabled",
  });
});

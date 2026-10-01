/**
 * What the agent editor holds and what it sends (06 — Agents and teams; 15
 * §Where it is on): a draft of an agent, and the body a save writes — a core
 * agent's three fields, an agent's whole definition, a new agent's. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_work/agentDraftModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { CORE_FIELDS, draftOf, emptyDraft, saveOf, harnessChoices, maySaveAgent, startingHarness } from "./agentDraftModel.mjs";

const core = readFileSync(new URL("../../../../crates/bisa-core/src/agent.rs", import.meta.url), "utf8");

const reviewer = {
  id: "reviewer",
  name: "Reviewer",
  photo: null,
  description: "Reads a change and says what is wrong with it.",
  system_prompt: "You review.",
  harness: "claude-code",
  models: { strategy: "fallback", models: [{ model: "claude-opus-5-5[1m]" }], effort: "high" },
  decision_making: true,
  respond: "members",
  skills: ["review"],
  mcps: ["github"],
  tags: ["engineering"],
  origin: { origin: "workspace" },
};

test("a core agent may change its harness, its model plan and its decision-making switch — the core's three, and nothing else is sent", () => {
  // The fields the core refuses by name on a core agent are every other one.
  const fixed = [...core.slice(core.indexOf("let changed: &[(&str, bool)] = &["), core.indexOf("if let Some((field, _)) = changed")).matchAll(/\("([a-z_]+)", next\./g)].map((m) => m[1]);
  assert.deepEqual(fixed, ["name", "photo", "description", "system_prompt", "skills", "mcps", "tags", "respond"]);
  assert.ok(core.includes("A core agent may change **only** its harness, its model"), "the rule, in the core's words");
  assert.deepEqual([...CORE_FIELDS], ["harness", "models", "decision_making"]);
  for (const field of fixed) assert.ok(!CORE_FIELDS.includes(field), `${field} is never sent for a core agent`);

  const general = { ...reviewer, id: "general-agent", name: "General Agent", origin: { origin: "core" }, decision_making: false };
  const draft = { ...draftOf(general), decision_making: true, name: "typed over by a stale draft", tags: ["stale"] };
  const save = saveOf(draft, general, true);
  assert.deepEqual(save, { kind: "patch", id: "general-agent", body: { harness: "claude-code", models: draft.plan, decision_making: true } });
  assert.deepEqual(Object.keys(save.body), [...CORE_FIELDS], "a stale draft is never a refusal naming a field nobody touched");
});

test("a description emptied in the editor reaches the node as null — the clearing `PATCH /agents/{id}` reads (06) — never as an empty string", () => {
  const cleared = saveOf({ ...draftOf(reviewer), description: "" }, reviewer, false);
  assert.equal(cleared.kind, "patch");
  assert.equal(cleared.body.description, null);
  assert.equal(saveOf({ ...draftOf(reviewer), description: "kept" }, reviewer, false).body.description, "kept");
  assert.ok(!("description" in saveOf({ ...emptyDraft(), name: "New", system_prompt: "p", harness: "claude-code" }, null, false).body), "a new agent with none sends no description at all");
});

test("an agent of yours is written whole, its two reference lists in their order; its switch rides with it", () => {
  const draft = { ...draftOf(reviewer), decision_making: false, skills: ["review", "security"] };
  assert.deepEqual(saveOf(draft, reviewer, false), {
    kind: "patch",
    id: "reviewer",
    body: {
      name: "Reviewer",
      photo: null,
      description: "Reads a change and says what is wrong with it.",
      system_prompt: "You review.",
      harness: "claude-code",
      models: draft.plan,
      decision_making: false,
      respond: "members",
      skills: ["review", "security"],
      mcps: ["github"],
      tags: ["engineering"],
    },
  });
  assert.equal(saveOf({ ...draft, description: "" }, reviewer, false).body.description, null, "a description cleared is none");
});

test("a new agent names what it has: no plan that names nothing, no switch that is off, no picture it lacks", () => {
  const blank = emptyDraft("claude-code");
  assert.deepEqual(blank, { name: "", photo: null, description: "", system_prompt: "", harness: "claude-code", plan: { strategy: "fallback", models: [] }, decision_making: false, respond: "owner_only", skills: [], mcps: [], tags: [] });
  const made = saveOf({ ...blank, name: "Scout", system_prompt: "You scout." }, null, false);
  assert.deepEqual(made, { kind: "create", body: { name: "Scout", system_prompt: "You scout.", harness: "claude-code", respond: "owner_only", skills: [], mcps: [], tags: [] } });
  const switched = saveOf({ ...blank, name: "Scout", decision_making: true, description: "Looks ahead.", plan: { strategy: "fallback", models: [], effort: "auto" } }, null, false);
  assert.equal(switched.body.decision_making, true, "Let the Decision-Making Agent decide for this agent");
  assert.deepEqual(switched.body.models, { strategy: "fallback", models: [], effort: "auto" }, "a plan that names no model may still name an effort");
  assert.equal(switched.body.description, "Looks ahead.");
});

test("a draft is the agent as it stands, its lists copied; its switch off when the agent names none", () => {
  const draft = draftOf(reviewer);
  assert.equal(draft.decision_making, true);
  assert.deepEqual(draft.plan, { strategy: "fallback", models: [{ model: "claude-opus-5-5[1m]" }], effort: "high" });
  assert.notEqual(draft.skills, reviewer.skills, "a copy: an edit never reaches the roster's row");
  assert.notEqual(draft.plan.models[0], reviewer.models.models[0]);
  assert.equal(draftOf({ ...reviewer, decision_making: undefined }).decision_making, false);
  assert.deepEqual(draftOf({ ...reviewer, models: undefined }).plan, { strategy: "fallback", models: [] });
  const editor = readFileSync(new URL("./AgentEditor.tsx", import.meta.url), "utf8");
  assert.ok(editor.includes("saveOf(d, agent, core)") && editor.includes("draftOf(agent)") && editor.includes("emptyDraft(startingAtOpen.current)"), "the editor asks the model");
});

test("a new agent starts on a harness this machine can run, or on none — no harness id is made up, and the picker shows what the definition holds", () => {
  const harnesses = [
    { id: "copilot", label: "GitHub Copilot CLI", installed: false },
    { id: "codex", label: "Codex", installed: true },
    { id: "grok", label: "Grok Build", installed: true },
  ];
  assert.equal(startingHarness(harnesses), "codex", "the first installed, in the node's order");
  assert.equal(startingHarness(harnesses.map((h) => ({ ...h, installed: false }))), null, "none installed: none picked, and none invented");
  assert.equal(startingHarness([]), null);
  assert.equal(startingHarness(null), null);
  assert.equal(emptyDraft().harness, null);
  assert.equal(emptyDraft(null).harness, null);
  assert.equal(emptyDraft("codex").harness, "codex");
  // The picker: the installed ones; an agent whose harness is not installed here keeps it, said so.
  assert.deepEqual(harnessChoices(harnesses, "codex"), [{ id: "codex", label: "Codex", installed: true }, { id: "grok", label: "Grok Build", installed: true }]);
  assert.deepEqual(harnessChoices(harnesses, "copilot").at(-1), { id: "copilot", label: "GitHub Copilot CLI (not installed here)", installed: false });
  assert.deepEqual(harnessChoices(harnesses, "elsewhere").at(-1), { id: "elsewhere", label: "elsewhere (not installed here)", installed: false }, "a harness this node does not list is said by its id");
  assert.deepEqual(harnessChoices([], null), []);
  // Save: a name, a prompt and a harness.
  const typed = { name: "Scout", system_prompt: "You scout.", harness: "codex" };
  assert.equal(maySaveAgent(typed, false), true);
  assert.equal(maySaveAgent(typed, true), false);
  assert.equal(maySaveAgent({ ...typed, harness: null }, false), false, "an agent nothing can run is never sent");
  assert.equal(maySaveAgent({ ...typed, name: "  " }, false), false);
  assert.equal(maySaveAgent({ ...typed, system_prompt: "" }, false), false);
  // The editor: seeded when it opens and never again while open; nothing in it names a harness.
  const editor = readFileSync(new URL("./AgentEditor.tsx", import.meta.url), "utf8");
  assert.ok(editor.includes("}, [open, agent]);"), "the harness list arriving does not take what was typed");
  assert.ok(editor.includes('prev.harness === null ? { ...prev, harness: starting } : prev'), "a draft with no harness takes the first installed, and nothing else of it moves");
  assert.ok(!/"claude-code"|"codex"|"copilot"|"grok"/.test(editor), "no harness id is spelt in the editor");
  assert.ok(editor.includes("disabled={!maySaveAgent(d, busy)}") && editor.includes("harnessChoices(harnesses, d.harness)"));
});


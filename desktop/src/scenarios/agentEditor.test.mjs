/**
 * The agent editor (06 — Agents and teams): wide enough for what it holds,
 * its skills and servers picked from the shared libraries side by side — a
 * skill is written under Settings › Skills, never here. Source guards, no DOM.
 *
 * Run with `node --test desktop/src/scenarios/agentEditor.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";

test("the agent editor is wide, picks skills and servers side by side, and writes no skill of its own", () => {
  const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  const editor = src("../views/_work/AgentEditor.tsx");
  assert.ok(editor.includes('width="max-w-4xl"'), "room for the model plan's rows and two pickers");
  assert.ok(editor.includes('<div className="grid gap-3 md:grid-cols-2">'), "skills and servers side by side");
  assert.equal((editor.match(/<RefPicker/g) ?? []).length, 2, "two pickers: skills, servers");
  assert.ok(!editor.includes("InlineSkillWriter") && !editor.includes("SkillFields") && !editor.includes("api.createSkill"), "no skill is written here");
  assert.ok(!editor.includes('from "../_settings/SkillsPanel"'), "a view never imports another panel's form");
  assert.ok(editor.includes('settingsSearch("skills")') && editor.includes('t("work-agent-editor-open-settings-skills")'), "the door to where a skill is written");
  assert.ok(editor.includes('settingsSearch("mcp")'), "and to where a server is registered");
  assert.ok(editor.includes('t("work-agent-editor-library-empty-write-first-skill-under")'), "an empty library points at Settings");
  const panel = src("../views/_settings/SkillsPanel.tsx");
  assert.ok(panel.includes("api.createSkill("), "Settings › Skills writes one");
  for (const name of ["SkillFields", "emptySkillDraft", "skillDraftReady"]) {
    assert.ok(!new RegExp(`^export (function|const) ${name}\\b`, "m").test(panel), `${name} is the panel's own`);
  }
});

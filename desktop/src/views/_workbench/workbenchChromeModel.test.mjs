import test from "node:test";
import assert from "node:assert/strict";
import { BOARD_COMMAND, MODE_COMMAND, modeSegments, modeSwitchLabel, modeWords, toggleWords } from "./workbenchChromeModel.mjs";
import { MODES } from "./ideModeModel.mjs";
import { COMMANDS } from "../../shell/keymapModel.mjs";

test("the mode switch names the three modes, each with a glyph and a real keymap chord — the cycle's, and the Board's own", () => {
  assert.deepEqual(modeWords("project"), { label: "Project", icon: "file", hint: "Documents and terminals in the centre", command: "toggle_ide_mode" });
  assert.deepEqual(modeWords("agent"), { label: "Agent", icon: "agent", hint: "The conversation with the agents in the centre", command: "toggle_ide_mode" });
  assert.deepEqual(modeWords("board"), { label: "Board", icon: "board", hint: "Every workstream as a card, by column, in the centre", command: "board" });
  assert.ok(COMMANDS.some((c) => c.id === MODE_COMMAND && c.when === "workbench"), "the cycle's command is a workbench keymap command");
  assert.ok(COMMANDS.some((c) => c.id === BOARD_COMMAND && c.when === "global"), "the Board's command works from anywhere");
  for (const m of MODES) assert.ok(COMMANDS.some((c) => c.id === modeWords(m).command), `${m}'s command is a keymap command`);
  assert.equal(modeSwitchLabel("project"), "Switch to Agent Mode");
  assert.equal(modeSwitchLabel("agent"), "Switch to Board Mode");
  assert.equal(modeSwitchLabel("board"), "Switch to Project Mode");
  assert.equal(modeSwitchLabel("agent", false), "Switch to Project Mode", "the Board off: the cycle skips it");
  assert.deepEqual(
    modeSegments().map((s) => s.id),
    ["project", "agent", "board"],
    "the switch reads Project · Agent · Board",
  );
  assert.deepEqual(modeSegments(false).map((s) => s.id), ["project", "agent"]);
  for (const s of modeSegments()) assert.equal(s.label, modeWords(s.id).label);
});

test("the two toggles say what pressing them does now, and each shows a real keymap chord", () => {
  assert.deepEqual(toggleWords("rail", true), { label: "Hide the project rail", command: "toggle_rail" });
  assert.deepEqual(toggleWords("rail", false), { label: "Show the project rail", command: "toggle_rail" });
  assert.deepEqual(toggleWords("right", true), { label: "Hide the right panel", command: "toggle_right_panel" });
  assert.deepEqual(toggleWords("right", false), { label: "Show the right panel", command: "toggle_right_panel" });
  const ids = new Set(COMMANDS.map((c) => c.id));
});

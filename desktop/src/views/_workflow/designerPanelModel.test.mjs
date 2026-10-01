/**
 * The Workflow Designer's right panel: three panes on a rail, one press
 * rule, a step picked showing its properties. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/designerPanelModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  DEFAULT_PANE,
  PANE_COMMAND,
  PANE_ICON,
  PANE_LABEL,
  PANEL_COMMAND,
  paneOfCommand,
  PANEL_PARAM,
  PANES,
  isPane,
  paneForSelection,
  paneOf,
  panelTabs,
  pressPane,
} from "./designerPanelModel.mjs";
import { COMMANDS } from "../../shell/keymapModel.mjs";

test("three panes, Properties first, each with a word, a glyph that exists and a keymap command; the column has its own", () => {
  assert.deepEqual([...PANES], ["properties", "agent", "runs"]);
  assert.equal(DEFAULT_PANE, "properties", "what you edit before who helps");
  assert.deepEqual(PANE_LABEL, { properties: "Properties", agent: "Agent", runs: "Runs" });
  const icons = readFileSync(new URL("../../ui/icons.ts", import.meta.url), "utf8");
  for (const pane of PANES) assert.match(icons, new RegExp(`^  ${PANE_ICON[pane]}: `, "m"), `${pane} wears a glyph that exists`);
  const ids = new Set(COMMANDS.map((c) => c.id));
  for (const pane of PANES) assert.ok(ids.has(PANE_COMMAND[pane]), `${pane}'s command ${PANE_COMMAND[pane]} is a keymap command`);
  assert.ok(ids.has(PANEL_COMMAND), "the column's toggle is a keymap command");
  const designer = COMMANDS.filter((c) => c.when === "designer").map((c) => c.id);
  assert.deepEqual(designer.sort(), [PANE_COMMAND.properties, PANE_COMMAND.agent, PANE_COMMAND.runs, PANEL_COMMAND].sort(), "the designer scope holds the panel's four commands and nothing else");
  assert.equal(PANEL_PARAM, "panel");
});

test("a stray value is Properties; a press opens, switches or closes", () => {
  assert.equal(isPane("agent"), true);
  assert.equal(isPane("runs"), true);
  assert.equal(isPane("workflow"), false, "the old mode is not a pane");
  assert.equal(paneOf("agent"), "agent");
  assert.equal(paneOf("nope"), "properties");
  assert.equal(paneOf(undefined), "properties");
  assert.deepEqual(pressPane({ open: false, tab: "properties" }, "agent"), { open: true, tab: "agent" });
  assert.deepEqual(pressPane({ open: true, tab: "properties" }, "agent"), { open: true, tab: "agent" });
  assert.deepEqual(pressPane({ open: true, tab: "agent" }, "agent"), { open: false, tab: "agent" });
  assert.deepEqual(pressPane({ open: false, tab: "agent" }, "nope"), { open: true, tab: "properties" }, "a stray target is Properties");
  assert.deepEqual(pressPane({ open: true, tab: "agent" }, "runs"), { open: true, tab: "runs" });
});

test("a step picked shows Properties — the column opens if it was closed or elsewhere; nothing picked changes nothing", () => {
  assert.deepEqual(paneForSelection({ open: false, tab: "agent" }, "build"), { open: true, tab: "properties" });
  assert.deepEqual(paneForSelection({ open: true, tab: "agent" }, "build"), { open: true, tab: "properties" });
  assert.deepEqual(paneForSelection({ open: true, tab: "runs" }, "build"), { open: true, tab: "properties" });
  const already = { open: true, tab: "properties" };
  assert.equal(paneForSelection(already, "build"), already, "the same state, by reference");
  const closed = { open: false, tab: "agent" };
  assert.equal(paneForSelection(closed, null), closed, "clearing a selection is not a wish to see anything");
});

test("the rail's tabs: Properties, Agent, Runs, the chosen one showing while the column is open — none is ever muted, a workflow exists before its designer opens", () => {
  const tabs = panelTabs({ open: true, tab: "agent" });
  assert.deepEqual(
    tabs.map((t) => [t.id, t.showing]),
    [
      ["properties", false],
      ["agent", true],
      ["runs", false],
    ],
  );
  assert.ok(tabs.every((t) => !("muted" in t) && !("mutedWords" in t)), "no tab has a muted state to carry");
  assert.deepEqual(
    panelTabs({ open: false, tab: "properties" }).map((t) => t.showing),
    [false, false, false],
    "a closed column marks nothing",
  );
  for (const tab of tabs) {
    assert.equal(tab.label, PANE_LABEL[tab.id]);
    assert.equal(tab.icon, PANE_ICON[tab.id]);
  }
});

test("a chord's command names its pane: the shell shows what the model says", () => {
  assert.deepEqual(PANES.map((pane) => paneOfCommand(PANE_COMMAND[pane])), [...PANES]);
  assert.equal(paneOfCommand("designer_runs"), "runs");
  assert.equal(paneOfCommand(PANEL_COMMAND), null, "the column's toggle names no pane");
  assert.equal(paneOfCommand("save"), null);
  assert.equal(paneOfCommand("toString"), null);
  const shell = readFileSync(new URL("../../shell/shortcuts.ts", import.meta.url), "utf8");
  assert.ok(shell.includes("showDesignerPane(pane)") && shell.includes("const pane = paneOfCommand(cmd);"), "the dispatch asks the model which pane");
  assert.equal(/cmd === "designer_/.test(shell), false, "and maps no command to a pane of its own");
  // The dispatch's arms are the model's commands, one each.
  const arms = [...shell.matchAll(/case "(designer_[a-z]+|toggle_designer_panel)":/g)].map((m) => m[1]);
  assert.deepEqual(arms.sort(), [...Object.values(PANE_COMMAND), PANEL_COMMAND].sort());
});

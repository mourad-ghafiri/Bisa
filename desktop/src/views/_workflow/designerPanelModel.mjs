/**
 * The Workflow Designer's right panel, as facts (03-workflows §The designer;
 * ide/02 §The right panel for the shape it mirrors).
 *
 * The canvas stays; beside it one column with a rail of three icon tabs at
 * the screen's right edge — **Properties** (the selected step's forms, or
 * the workflow's own: name, description, tags, inputs, problems), **Agent**
 * (the conversations about this workflow, where `@Workflow Agent` reads,
 * validates and saves it) and **Runs** (its runs of the workspace, live
 * first then newest, with *Run…* on top). A press on the rail is every rail's one rule
 * (`ui/iconRailModel.mjs`: open · switch · close); a step picked on the
 * canvas shows Properties, since a pick is a wish to see the step. Every
 * workflow the designer opens is stored — a new one is created before its
 * designer opens — so both tabs are always there to press. Which tab shows
 * and whether the column is open are furniture, remembered once for the
 * machine (`designerPanelStore.ts`).
 */

import { pressRailTab } from "../../ui/iconRailModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The three panes, in rail order: what you edit, who helps, what it did. */
export const PANES = Object.freeze(["properties", "agent", "runs"]);

export const DEFAULT_PANE = "properties";

export const PANE_LABEL = Object.freeze({
  properties: t("workflow-designer-panel-properties"),
  agent: t("workflow-designer-panel-agent"),
  runs: t("workflow-designer-panel-runs"),
});

/** Each pane's glyph, a name in `ICON`. */
export const PANE_ICON = Object.freeze({ properties: "settings", agent: "agent", runs: "run" });

/** The keymap command that shows each pane, and the one that shows or hides the column. */
export const PANE_COMMAND = Object.freeze({ properties: "designer_properties", agent: "designer_agent", runs: "designer_runs" });
export const PANEL_COMMAND = "toggle_designer_panel";

/** The pane a keymap command shows, or `null` for a command that names none — the column's toggle among them. @param {unknown} command */
export function paneOfCommand(command) {
  return PANES.find((pane) => PANE_COMMAND[pane] === command) ?? null;
}

/** The address key a link lands on a pane with: `?panel=agent`. */
export const PANEL_PARAM = "panel";

/** @param {unknown} value */
export function isPane(value) {
  return typeof value === "string" && PANES.includes(value);
}

/** A stored or typed pane made safe to render. @param {unknown} value */
export function paneOf(value) {
  return isPane(value) ? value : DEFAULT_PANE;
}

/**
 * A rail tab pressed: open · switch · close.
 * @param {{open: boolean, tab: string}} state
 * @param {string} target
 */
export function pressPane(state, target) {
  return pressRailTab(state, paneOf(target));
}

/**
 * A step picked on the canvas shows Properties — the column opens if it was
 * closed. Nothing picked leaves the panel as it is: clearing a selection is
 * not a wish to see anything else.
 * @param {{open: boolean, tab: string}} state
 * @param {string | null} selected
 */
export function paneForSelection(state, selected) {
  if (!selected) return state;
  if (state.open && state.tab === "properties") return state;
  return { open: true, tab: "properties" };
}

/**
 * The rail's tabs for a screen: which one shows.
 * @param {{open: boolean, tab: string}} screen
 * @returns {{id: string, label: string, icon: string, showing: boolean}[]}
 */
export function panelTabs({ open, tab }) {
  return PANES.map((id) => ({ id, label: PANE_LABEL[id], icon: PANE_ICON[id], showing: open && tab === id }));
}

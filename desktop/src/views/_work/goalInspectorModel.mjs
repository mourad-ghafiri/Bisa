/**
 * What the goal's Details pane says, decided before it draws: which panels
 * it offers and which the address names, the tabs with their glyphs and the
 * turn each yields its word in, the run's card in one line, what the goal
 * has spent, how many projects it carries, where the Workflow Agent stands,
 * and where the goal came from — a capture, the goal it refines, or the step
 * of a run in the workspace that made it. `GoalInspector.tsx` paints; a Work
 * row's line is `workItemRowModel.mjs`'s.
 *
 * **Properties** is the pane's fifth panel, offered while the goal's centre
 * tab is Workflow: the designer's own pane (`views/_workflow/Inspector.tsx`),
 * which the Workflow tab draws into the pane through a slot the panel
 * publishes (`_goal/propertiesSlot.ts`) — so the goal screen has one right
 * pane, and a step, a flow or a box picked on the canvas shows it, opening
 * the pane if it was closed, by the designer's own rule
 * (`designerPanelModel.paneForSelection`).
 */

import { cents } from "../../activityModel.mjs";
import { number } from "../../i18n/format.mjs";
import { t } from "../../i18n/l10n.mjs";
import { MODE_LABEL } from "../_goal/goalMode.mjs";
import { cancelWords, runIndex } from "../_goal/runControl.mjs";
import { paneForSelection } from "../_workflow/designerPanelModel.mjs";
import { stepOf } from "../_workflow/selectionModel.mjs";

/** The pane's panels, in the order its tabs draw them; the first is the one a bare address shows. */
export const PANELS = Object.freeze(["details", "work", "projects", "files"]);

/** The fifth panel — the Workflow tab's own, the designer's word for it. */
export const PROPERTIES = "properties";

/** Every panel, Properties last, so the four never shift when it appears. */
const WITH_PROPERTIES = Object.freeze([...PANELS, PROPERTIES]);

/**
 * The panels the pane offers: the four, and Properties while the goal's
 * centre tab is Workflow — the one tab whose canvas has something to pick.
 * @param {{properties?: boolean}} facts
 * @returns {readonly string[]}
 */
export function panelsFor({ properties = false } = {}) {
  return properties ? WITH_PROPERTIES : PANELS;
}

/**
 * The panel the address names — `?insp=` — while it is offered, or Details
 * for anything else: a stale `?insp=properties` on the Progress tab reads as
 * Details, and comes back when the Workflow tab does, with no URL rewrite.
 * @param {unknown} raw
 * @param {readonly string[]} [offered] what the pane offers now (`panelsFor`); the four by default
 * @returns {"details" | "work" | "projects" | "files" | "properties"}
 */
export function panelOf(raw, offered = PANELS) {
  return offered.includes(raw) ? raw : PANELS[0];
}

/** Each panel's glyph, a name in `ICON`: Details wears the ⓘ toggle's, Properties the designer rail's. */
export const PANEL_ICON = Object.freeze({ details: "info", work: "workItem", projects: "project", files: "file", properties: "settings" });

/**
 * The turn in which each tab yields its word when the strip is too tight
 * (`ui/Tabs.tsx`, `tabsFitModel`): from the edge in — Files first, then
 * Projects, Work, Properties — and Details, the pane's home, keeps its word
 * longest. Four text tabs fit a 300px pane; five do not, and the strip clips
 * rather than wraps, so every tab wears a glyph and folds to it in turn.
 */
export const PANEL_FOLD = Object.freeze({ files: 1, projects: 2, work: 3, properties: 4, details: 5 });

const PANEL_LABEL = Object.freeze({
  details: t("work-goal-inspector-details"),
  work: t("work-goal-inspector-work"),
  projects: t("work-goal-inspector-projects"),
  files: t("work-goal-inspector-files"),
  properties: t("work-goal-inspector-properties"),
});

/**
 * The strip's tabs for the panels offered: each with its word, its glyph,
 * its fold turn, and — on Work — what its badge counts, for the folded
 * tab's tooltip.
 * @param {readonly string[]} offered
 * @param {{work?: number}} [counts]
 * @returns {{id: string, label: string, icon: string, fold: number, count: number | null}[]}
 */
export function panelTabs(offered, counts = {}) {
  return offered.map((id) => ({ id, label: PANEL_LABEL[id], icon: PANEL_ICON[id], fold: PANEL_FOLD[id], count: id === "work" ? (counts.work ?? null) : null }));
}

/**
 * What a pick on the canvas does to the pane: the address patch that shows
 * Properties — opening the pane if it was closed or on another occupant,
 * switching it if it was on another panel — or `null` when nothing changes:
 * nothing picked, or Properties already showing. The designer's rule
 * (`paneForSelection`), read against the pane's two address keys.
 * @param {{kind: string | null | undefined, panel: unknown}} showing the pane's occupant (`?aux=`) and panel (`?insp=`)
 * @param {unknown} selection
 * @returns {{aux: "inspector", auxId: null, insp: "properties"} | null}
 */
export function propertiesReveal(showing, selection) {
  const state = { open: showing.kind === "inspector", tab: panelOf(showing.panel, WITH_PROPERTIES) };
  return paneForSelection(state, selection) === state ? null : { aux: "inspector", auxId: null, insp: PROPERTIES };
}

/**
 * The name the Properties scrollport keeps its place under: each step's page
 * its own, the workflow's the bare name — spelt as the tab always spelt it.
 * @param {unknown} selection
 */
export function propertiesScrollName(selection) {
  return `workflow:inspector:${stepOf(selection) ?? ""}`;
}

/**
 * What the goal has spent, in one line: tokens, cost, seconds.
 * @param {{tokens?: number, usd_cents?: number, wall_clock_secs?: number} | null | undefined} spent
 */
export function spendWords(spent) {
  return t("work-goal-inspector-spent", { tokens: number(Number(spent?.tokens) || 0), cost: cents(Number(spent?.usd_cents) || 0), secs: Number(spent?.wall_clock_secs) || 0 });
}

/**
 * The line under the run's card: its number among the goal's runs, the
 * revision it froze, and how it ended when it has.
 * @param {{id: string, workflow: {revision: number}, outcome?: string | null, cancelled?: object | null}} run
 * @param {readonly {id: string}[] | null | undefined} runs
 */
export function runCardWords(run, runs) {
  const total = (runs ?? []).length;
  const line = t("work-goal-inspector-run-of", { index: runIndex(runs, run.id) ?? total, total, revision: run.workflow.revision });
  const ended = run.cancelled ? cancelWords(run.cancelled) : run.outcome === "done" ? t("goal-run-control-word-done") : run.outcome === "failed" ? t("goal-run-control-word-failed") : null;
  return ended ? t("work-goal-inspector-run-ended", { line, ended }) : line;
}

/** How many of a goal's projects the card names before it counts the rest. */
export const PROJECTS_NAMED = 3;

/**
 * The projects card's two lines: the first few by name with the rest
 * counted, then how many projects and workstreams there are.
 * @param {readonly {project: {name: string}, workstreams?: number}[] | null | undefined} rows
 * @returns {{names: string, counts: string} | null} `null` for a goal with none
 */
export function projectsWords(rows) {
  const all = rows ?? [];
  if (all.length === 0) return null;
  const named = all.slice(0, PROJECTS_NAMED).map((r) => r.project.name).join(" · ");
  const rest = all.length - PROJECTS_NAMED;
  return {
    names: rest > 0 ? `${named} +${rest}` : named,
    counts: t("work-goal-inspector-projects-workstreams", { projects: all.length, workstreams: all.reduce((n, r) => n + (Number(r.workstreams) || 0), 0) }),
  };
}

/**
 * Where the Workflow Agent stands on a goal it designs for, in one line —
 * designing, repairing, or that designing is off on this node and nobody is
 * at it. `null` between the two: nothing to say.
 * @param {string} mode
 * @param {{design_enabled?: boolean, phase?: string | null} | null | undefined} guidance
 */
export function designLine(mode, guidance) {
  const word = MODE_LABEL[mode] ?? String(mode);
  if (!guidance?.design_enabled) return t("work-goal-inspector-goal-but-designing-off-node-nobody", { mode: word.toLowerCase() });
  if (guidance.phase === "design") return t("work-goal-inspector-workflow-agent-designing-workflow");
  if (guidance.phase === "repair") return t("work-goal-inspector-workflow-agent-repairing-after-failed-step");
  return null;
}

/**
 * Where the goal came from, as the chips under *Origin*: its mode, then the
 * goal it refines, or the run in the workspace whose step made it — each a
 * door — then the run it is on. An origin is history: the door may lead to
 * a thing that has gone since, and the page there says so.
 * @param {{mode?: string, origin?: {origin?: string, parent?: string, run?: string, step?: string}}} goal
 * @param {{id: string, workflow: {name: string}} | null | undefined} run
 * @param {string} mode the goal's mode, read once by the caller (`goalMode.modeOf`)
 * @returns {{id: string, label: string, icon: string, title?: string, route?: {name: string, id: string}}[]}
 */
export function originChips(goal, run, mode) {
  const chips = [{ id: "mode", label: MODE_LABEL[mode] ?? String(mode), icon: "mode" }];
  const origin = goal?.origin ?? {};
  if (origin.origin === "spawned" && origin.parent) chips.push({ id: "parent", label: t("work-goal-inspector-refines-goal"), icon: "goal", route: { name: "goal", id: origin.parent } });
  if (origin.origin === "run" && origin.run) chips.push({ id: "born", label: t("work-goal-inspector-made-by-run-step", { step: origin.step ?? "" }), icon: "run", title: t("work-goal-inspector-made-by-run-step-title"), route: { name: "run", id: origin.run } });
  if (run) chips.push({ id: "run", label: t("work-goal-inspector-run-named", { run: run.id.slice(-6), workflow: run.workflow.name }), icon: "run" });
  return chips;
}

/**
 * The word a checkout wears in the Projects panel: the name a person gave
 * it, else its branch, else — for a copy — that it is one, by the tail of
 * its id.
 * @param {{id: string, name?: string | null, kind: {kind: string, branch?: string}}} workstream
 */
export function workstreamWord(workstream) {
  if (workstream.name) return workstream.name;
  if (workstream.kind.kind === "worktree" && workstream.kind.branch) return workstream.kind.branch;
  return t("work-goal-inspector-copy", { id: workstream.id.slice(-6) });
}

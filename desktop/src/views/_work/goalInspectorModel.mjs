/**
 * What the goal's Details pane says, decided before it draws: which panel
 * the address names, the run's card in one line, what the goal has spent,
 * how many projects it carries, where the Workflow Agent stands, and where
 * the goal came from — a capture, the goal it refines, or the step of a run
 * in the workspace that made it. `GoalInspector.tsx` paints; a Work row's
 * line is `workItemRowModel.mjs`'s.
 */

import { cents } from "../../activityModel.mjs";
import { number } from "../../i18n/format.mjs";
import { t } from "../../i18n/l10n.mjs";
import { MODE_LABEL } from "../_goal/goalMode.mjs";
import { cancelWords, runIndex } from "../_goal/runControl.mjs";

/** The pane's panels, in the order its tabs draw them; the first is the one a bare address shows. */
export const PANELS = Object.freeze(["details", "work", "projects", "files"]);

/**
 * The panel the address names — `?insp=` — or Details for anything else.
 * @param {unknown} raw
 * @returns {"details" | "work" | "projects" | "files"}
 */
export function panelOf(raw) {
  return PANELS.includes(raw) ? raw : PANELS[0];
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

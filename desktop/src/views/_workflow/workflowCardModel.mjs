/**
 * A library card's facts as words (03-workflows §The library): one state
 * line — what matters most about the workflow right now — with an *On*
 * mark beside it while it listens for its events, one line of dimmed facts,
 * and the shape of its `⋮` menu. From the row alone; the card draws, the
 * designer's header reads the same verbs.
 */

import { t } from "../../i18n/l10n.mjs";
import { liveGoals } from "./workflowVerbs.mjs";

/**
 * The one state a card leads with, in the order that matters: put away
 * beats everything (nothing runs an archived workflow); a run in motion
 * beats a problem (a run's copy is frozen; the problem is the draft's) —
 * its runs of the workspace first, then the goals running it; a problem
 * beats the one reason it runs on a goal only (a step reads `{goal.…}`);
 * that beats readiness.
 * @param {import("../../types").WorkflowRow} row
 * @returns {{tone: "quiet" | "accent" | "danger" | "ok", icon: string, words: string, live: boolean}}
 */
export function cardStatus(row) {
  const w = row.workflow;
  if (w.archived) return { tone: "quiet", icon: "archive", words: t("workflow-workflow-card-archived"), live: false };
  const runs = row.runs?.live ?? 0;
  if (runs > 0) return { tone: "accent", icon: "run", words: t("workflow-workflow-card-running-runs", { n: runs }), live: true };
  const goals = liveGoals(row.used_by).length;
  if (goals > 0) return { tone: "accent", icon: "run", words: t("workflow-workflow-card-running-in-goals", { n: goals }), live: true };
  const problems = (row.problems ?? []).length;
  if (problems > 0) return { tone: "danger", icon: "warn", words: t("workflow-workflow-card-problem-s", { problems }), live: false };
  if ((row.workspace_problems ?? []).length > 0) return { tone: "quiet", icon: "goal", words: t("workflow-workflow-card-runs-on-goal"), live: false };
  return { tone: "ok", icon: "ok", words: t("workflow-workflow-card-ready-to-run"), live: false };
}

/**
 * The mark beside the state line while the workflow is On: *On*, or
 * *Paused* when a failure or a spent budget stopped it hearing its events.
 * `null` while it is Off.
 * @param {import("../../types").WorkflowRow} row
 * @returns {{words: string, tone: "ok" | "warn"} | null}
 */
export function onMark(row) {
  const listening = row?.listening;
  if (!listening) return null;
  return listening.paused ? { words: t("workflow-workflow-card-paused"), tone: "warn" } : { words: t("workflow-workflow-card-on"), tone: "ok" };
}

/**
 * The dimmed facts under the description — how big, where from — joined by
 * the card with ` · `.
 * @param {import("../../types").WorkflowRow} row
 * @returns {string[]}
 */
export function cardMeta(row) {
  const w = row.workflow;
  const out = [t("workflow-goal-workflow-tab-steps", { steps: w.steps.length })];
  const inputs = (w.inputs ?? []).length;
  if (inputs > 0) out.push(t("workflow-workflow-card-inputs", { inputs }));
  out.push(w.origin.origin === "catalog" ? t("workflow-workflow-card-from-template", { slug: w.origin.slug }) : t("workflow-workflow-card-yours"));
  return out;
}

/**
 * The `⋮`'s items, by id: *Open* and *Delete…* always; *Run…* (*Test
 * run…*) when the verbs offer it (it can start in the workspace); *Turn
 * on…* or *Turn off* when it has an event start to hear; *Restart every
 * run* and *Stop every run* while a run of it in the workspace goes. The
 * first verb opens the group. The card binds each id to its door or dialog.
 * @param {import("./workflowVerbs.mjs").WorkflowVerbs} verbs
 * @returns {{id: "open" | "run" | "turn_on" | "turn_off" | "restart" | "stop" | "delete", label: string, danger?: boolean, separatorBefore?: boolean}[]}
 */
export function cardMenu(verbs) {
  const items = [{ id: "open", label: t("workflow-workflow-card-open") }];
  const verbItems = [];
  if (verbs.run) verbItems.push({ id: "run", label: verbs.run.label });
  if (verbs.turnOn) verbItems.push({ id: "turn_on", label: verbs.turnOn.label });
  if (verbs.turnOff) verbItems.push({ id: "turn_off", label: verbs.turnOff.label });
  if (verbs.restart) verbItems.push({ id: "restart", label: verbs.restart.label });
  if (verbs.stop) verbItems.push({ id: "stop", label: verbs.stop.label, danger: true });
  verbItems.forEach((item, i) => items.push({ ...item, separatorBefore: i === 0 }));
  items.push({ id: "delete", label: t("workflow-workflow-card-delete"), danger: true, separatorBefore: true });
  return items;
}

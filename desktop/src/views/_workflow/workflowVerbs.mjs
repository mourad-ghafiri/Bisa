/**
 * The verbs a library workflow offers on its card and in the designer's
 * menu — *Run…* (or *Test run…* for a workflow only events begin), *Turn
 * on…* / *Turn off*, *Stop every run*, *Restart every run* — and the words
 * the confirms say. From the row alone: its problems, why it cannot run in
 * the workspace (`workspace_problems`), whether it is archived, whether only
 * events begin it (`event_only`), what it starts on (`starts`), whether it
 * is On (`listening`), and how many of its runs of the workspace are going
 * (`runs.live`). *Run…* starts a run of the workspace — no goal is captured;
 * stop and restart act on its runs of the workspace alone: a goal's run of
 * the workflow is its goal's.
 */

import { t } from "../../i18n/l10n.mjs";
import { hasEventStarts, turnOnBlockers } from "./listeningModel.mjs";

/**
 * The goals in motion on this workflow, from its holders — what freezes the
 * designer while a goal's run is of it.
 */
export function liveGoals(usedBy) {
  return (usedBy ?? []).filter((r) => r.kind === "goal" && r.live);
}

/**
 * The verbs, or `null` for each not offered. An archived workflow runs
 * nothing; one with problems, or one that reads a goal it would not have
 * in the workspace, cannot start there — and a workflow only events begin
 * starts by hand only as a test run, *as if* one of its events happened.
 * Turning On needs an event start and nothing in the way (`turnOnBlockers`);
 * turning Off, only that it is On. Stop and restart need a run of the
 * workspace that is going.
 */
export function workflowVerbs(row) {
  const none = { run: null, stop: null, restart: null, turnOn: null, turnOff: null };
  if (!row) return none;
  const w = row.workflow ?? {};
  const live = row.runs?.live ?? 0;
  const runs = !w.archived && (row.problems ?? []).length === 0 && (row.workspace_problems ?? []).length === 0;
  const test = !!row.event_only;
  return {
    run: runs ? { label: test ? t("workflow-workflow-verbs-test-run") : t("workflow-workflow-verbs-run"), test } : null,
    stop: live > 0 ? { label: t("workflow-workflow-card-stop-every-run"), live } : null,
    restart: live > 0 && !w.archived ? { label: t("workflow-workflow-card-restart-every-run"), live } : null,
    turnOn: hasEventStarts(row) && !row.listening && turnOnBlockers(row).length === 0 ? { label: t("workflow-workflow-verbs-turn-on") } : null,
    turnOff: row.listening ? { label: t("workflow-workflow-verbs-turn-off") } : null,
  };
}

/** What stopping every run does, for the confirm. */
export function stopEveryWords(live) {
  return t("workflow-workflow-verbs-stop-every-run-words", { n: live });
}

/** What restarting every run does, for the confirm. */
export function restartEveryWords(live) {
  return t("workflow-workflow-verbs-restart-every-run-words", { n: live });
}

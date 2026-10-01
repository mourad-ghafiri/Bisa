/**
 * A goal's run verbs, and the words around them.
 *
 * One rule for every surface that offers a start, a stop or a restart — the
 * goal page's header and its Progress tab, the Workflow tab's bar, the Goals
 * list's card menu — so none of them offers a start over the Workflow Agent
 * while it designs or repairs, none offers a restart of a goal that never
 * ran, and a stop is offered exactly while something is live or queued.
 *
 * A goal has at most one **live** run — started and unfinished. A run made
 * while one is live is **queued**: it waits its turn and starts on its own
 * when the live run ends. A **stop** cancels the live run and withdraws the
 * queue; the goal stays open, ready for a new run. A **restart** cancels the
 * live run and starts a new run of the same workflow with the same inputs,
 * ahead of the queue. A queued run may be **withdrawn** on its own.
 *
 * The same liveness freezes the goal screen's right panel (`panelFrozen`):
 * while a run goes, nothing about the goal is edited — its assignees,
 * documents and files wait for the run to finish, and so does its
 * workflow: the desktop never amends a running workflow. A queue freezes
 * nothing on its own.
 *
 * Plain `.mjs` with a `.d.mts` beside it: the rule is a fact a person acts on,
 * and `node --test` reaches it here.
 */

import { designInProgress } from "./designStatus.mjs";
import { t } from "../../i18n/l10n.mjs";

/** A run is live once started and until it has an outcome or was cancelled. */
export function runIsLive(run) {
  if (!run) return false;
  return run.started_at != null && run.outcome == null && !run.cancelled;
}

/** A run is queued until it starts: no start, no outcome, not cancelled. */
export function runIsQueued(run) {
  if (!run) return false;
  return run.started_at == null && run.outcome == null && !run.cancelled;
}

/** A run summary is live while its status is `running` or `waiting`. */
export function summaryIsLive(summary) {
  return !!summary && (summary.status === "running" || summary.status === "waiting");
}

/** A run summary is queued while its status says so. */
export function summaryIsQueued(summary) {
  return !!summary && summary.status === "queued";
}

/**
 * Whether any run of the goal is live: the current one, or any in the list —
 * the list is the safety net when the goal's pointer lags a write. A queued
 * run is not live.
 */
export function anyRunLive(run, runs) {
  return runIsLive(run) || (Array.isArray(runs) && runs.some(summaryIsLive));
}

/** The right panel is read-only while any run of the goal is live. */
export function panelFrozen(run, runs) {
  return anyRunLive(run, runs);
}

/** The line the frozen panel shows in place of its controls. */
export const FROZEN_HINT = t("goal-run-control-run-going-goal-s-details-workflow");

/** The id of the goal's live run, from the current run or the list; `null` when none is. */
export function liveRun(run, runs) {
  if (runIsLive(run)) return run.id;
  const found = (runs ?? []).find(summaryIsLive);
  return found ? found.id : null;
}

/** The queued runs in the order they start, first next. */
export function queuedRuns(runs) {
  return (runs ?? []).filter(summaryIsQueued).sort((a, b) => (a.position ?? 0) - (b.position ?? 0));
}

/** The run summaries oldest first — the one order every numbering reads. */
function oldestFirst(runs) {
  return [...(runs ?? [])].sort((a, b) => (a.queued_at ?? 0) - (b.queued_at ?? 0) || String(a.id).localeCompare(String(b.id)));
}

/**
 * A run's number on its goal: the oldest is 1. The one numbering the
 * overlay, the history and the inspector share; `null` for a run the list
 * does not hold.
 */
export function runIndex(runs, id) {
  const i = oldestFirst(runs).findIndex((r) => r.id === id);
  return i < 0 ? null : i + 1;
}

/**
 * The verbs a goal offers, or `null` for each it does not. `proposed` is the
 * Workflow Agent's proposal awaiting adoption; `startable` says a workflow
 * exists to run at all. A closed goal offers nothing. Designing blocks a
 * start and a restart, never a stop. A proposal offers *Adopt and start…*
 * and no restart. A stop is offered whenever a run is live or queued.
 *
 * A design that begins on events (`listens`) is started by listening: its
 * goal's start arms its start events — *Start listening…* — and each
 * occurrence runs it. Once the goal listens, a run by hand is *Run now…* at
 * the design's start by hand (`manualEntry`), and none when only events
 * begin it; *Stop listening* and *Listen again* are `listenVerb`'s.
 */
export function runVerbs({ goal, run, runs, guidance, proposed, startable, listens = false, manualEntry = null }) {
  const none = { start: null, stop: null, restart: null };
  if (!goal || goal.closed) return none;
  const live = anyRunLive(run, runs);
  const queued = queuedRuns(runs).length;
  const designing = designInProgress(guidance, goal);
  const stop = live || queued > 0 ? { label: t("goal-run-verb-dialogs-stop"), live, queued } : null;
  let start = null;
  if (startable && !designing) {
    if (proposed) start = { label: t("goal-run-control-adopt-start"), queues: false, adopt: true };
    else if (listens && !goal.listening) start = { label: t("goal-run-control-start-listening"), queues: false, adopt: false, listen: true };
    else if (listens) start = manualEntry ? { label: t("goal-run-control-run-now"), queues: live, adopt: false, at: manualEntry } : null;
    else if (live) start = { label: t("goal-progress-tab-new-run"), queues: true, adopt: false };
    else if (run) start = { label: t("goal-progress-tab-new-run"), queues: false, adopt: false };
    else start = { label: t("goal-run-control-start-run"), queues: false, adopt: false };
  }
  const restart = run && startable && !proposed && !designing ? { label: t("goal-progress-tab-restart") } : null;
  return { start, stop, restart };
}

/**
 * The verb a listening goal's header line offers: *Stop listening* while it
 * listens, *Listen again* once a failed run or a spent budget paused it —
 * `null` for a goal that does not listen, and for a closed one.
 */
export function listenVerb(goal) {
  const listening = goal?.listening;
  if (!listening || goal.closed) return null;
  return listening.paused ? { id: "again", label: t("goal-run-control-listen-again") } : { id: "stop", label: t("goal-run-control-stop-listening") };
}

/** The word for why a run was cancelled, from its cause. */
export function cancelWords(cause) {
  switch (cause?.cause) {
    case "stopped":
      return t("goal-run-control-cause-stopped");
    case "restarted":
      return t("goal-run-control-cause-restarted");
    case "withdrawn":
      return t("goal-run-control-cause-withdrawn");
    case "closed":
      return t("goal-run-control-cause-closed");
    case "retired":
      return t("goal-run-control-cause-retired");
    default:
      return t("goal-run-control-word-cancelled");
  }
}

/**
 * A run's status, read off the run itself — the core's projection
 * (`WorkflowRun::status`), restated: a cancel outranks an outcome; a run
 * not started is queued; a started one is `running` while any step runs and
 * `waiting` while every live step waits. `null` for no run.
 * @returns {"queued" | "running" | "waiting" | "done" | "failed" | "cancelled" | null}
 */
export function runStatus(run) {
  if (!run) return null;
  if (run.cancelled) return "cancelled";
  if (run.outcome === "done" || run.outcome === "failed") return run.outcome;
  if (run.started_at == null) return "queued";
  return Object.values(run.steps ?? {}).some((r) => r?.state?.state === "running") ? "running" : "waiting";
}

/**
 * A run summary in words: the status word (with its place when queued, its
 * cause when cancelled), the theme tone it wears, the time that matters and
 * what that time is.
 */
export function runWords(summary) {
  const s = summary ?? {};
  switch (s.status) {
    case "queued": {
      const position = s.position ?? 1;
      return { word: position === 1 ? t("goal-run-control-queued-next-line") : t("goal-run-control-queued-line", { position }), tone: "quiet", at: s.queued_at ?? null, atWord: t("goal-run-control-at-queued") };
    }
    case "running":
      return { word: t("goal-run-control-word-running"), tone: "accent", at: s.started_at ?? null, atWord: t("goal-run-control-at-started") };
    case "waiting":
      return { word: t("goal-run-control-word-waiting"), tone: "warn", at: s.started_at ?? null, atWord: t("goal-run-control-at-started") };
    case "done":
      return { word: t("goal-run-control-word-done"), tone: "ok", at: s.finished_at ?? null, atWord: t("goal-run-control-at-finished") };
    case "failed":
      return { word: t("goal-run-control-word-failed"), tone: "danger", at: s.finished_at ?? null, atWord: t("goal-run-control-at-finished") };
    case "cancelled":
      return { word: cancelWords(s.cause), tone: "quiet", at: s.finished_at ?? s.queued_at ?? null, atWord: t("goal-run-control-at-ended") };
    default:
      // A status this build has no word for is said as the node said it.
      return { word: String(s.status ?? "unknown"), tone: "quiet", at: s.queued_at ?? null, atWord: t("goal-run-control-at-made") }; // for the machine
  }
}

/** What a stop does, for the confirm. */
export function stopWords({ live, queued, liveSteps }) {
  const parts = [];
  if (live) {
    const n = liveSteps ?? 0;
    parts.push(n > 0 ? t("goal-run-control-live-run-cancelled-live-work-stops", { n }) : t("goal-run-control-live-run-cancelled"));
  }
  if (queued > 0) parts.push(t("goal-run-control-queued-withdrawn", { queued }));
  parts.push(t("goal-run-control-goal-stays-open-ready-new-run"));
  return parts.join(" ");
}

/**
 * What a restart does, for the confirm — or `null` when nothing is live and
 * no confirm is owed: a restart of a finished goal only starts a run.
 */
export function restartWords({ live, queued }) {
  if (!live) return null;
  return t("goal-run-control-live-run-cancelled-new-run-same", { queued: queued ?? 0 });
}

/**
 * The goal's runs as rows, newest first: the number, the words, whether the
 * row offers *Withdraw* — a queued run's alone — and whether the run is
 * still to end (`live`: going, or queued behind the one that is), which a
 * list drawn within a bound never leaves out.
 */
export function runRows(runs) {
  const ordered = oldestFirst(runs);
  return ordered
    .map((r, i) => ({
      id: r.id,
      index: r.number ?? i + 1,
      status: r.status,
      revision: r.revision,
      workflow: r.workflow,
      words: runWords(r),
      withdraw: summaryIsQueued(r),
      live: summaryIsLive(r) || summaryIsQueued(r),
    }))
    .reverse();
}

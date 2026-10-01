/**
 * A workflow's runs of the workspace as the designer's Runs pane and a run's
 * own page read them (03-workflows §Runs of the workspace): the rows in the
 * order that matters — the ones still going first, the ones a person may
 * still stop, then the newest — each with its title (its workflow's name and
 * its number), its status in words, who started it, and the verbs it
 * admits. From the node's `RunSummary` rows alone; the status words are the
 * goal's run list's (`runControl.runWords`). Plain `.mjs`, so `node --test`
 * reads it.
 */

import { t } from "../../i18n/l10n.mjs";
import { runStanding } from "../_goal/goalPageModel.mjs";
import { finishedWords } from "../_goal/progressModel.mjs";
import { runWords } from "../_goal/runControl.mjs";
import { HOLDER_LABEL } from "../_goals/goalStripModel.mjs";
import { readWords } from "../_settings/loadModel.mjs";
import { currentSteps } from "./runView.mjs";

const LIVE = new Set(["queued", "running", "waiting"]);

/** Whether a run of this status is still going — queued, running or waiting. @param {string} status */
export function isLive(status) {
  return LIVE.has(status);
}

/**
 * The rows in the order a person reads them: the runs still going, then the
 * rest — each group newest first, as the node answers them (a stable sort
 * keeps its order within each).
 * @param {readonly {status: string}[] | null | undefined} runs
 */
export function orderRuns(runs) {
  return [...(runs ?? [])].sort((a, b) => Number(isLive(b.status)) - Number(isLive(a.status)));
}

/** A run's title: its workflow's name and its number — *Nightly report #3*. @param {{workflow_name: string, number: number}} summary */
export function runTitle(summary) {
  return t("workflow-runs-run-title", { name: summary.workflow_name, number: summary.number });
}

/**
 * The words for a run an event started, by the kind of event its start
 * listens for — with the detail the node gives after the word it qualifies:
 * who sent the message, which signal, which run.
 */
const EVENT_WORDS = Object.freeze({
  schedule: () => t("workflow-runs-started-by-schedule"),
  hook: () => t("workflow-runs-started-by-hook"),
  message: (detail) => (detail ? t("workflow-runs-started-by-message-from", { detail }) : t("workflow-runs-started-by-message")),
  signal: (detail) => (detail ? t("workflow-runs-started-by-signal-named", { detail }) : t("workflow-runs-started-by-signal")),
  run: (detail) => (detail ? t("workflow-runs-started-by-run-named", { detail }) : t("workflow-runs-started-by-run")),
  project: () => t("workflow-runs-started-by-project"),
  platform: () => t("workflow-runs-started-by-platform"),
  connector: () => t("workflow-runs-started-by-connector"),
  check: () => t("workflow-runs-started-by-check"),
});

/**
 * Who started it (`RunSummary.started_by`): a person's *Run…*, an event one
 * of the workflow's start events heard — *by schedule*, *by message from
 * Maya*, *by signal report.ready*, *by run #4* — or a test run. A summary
 * that says nothing reads as a person's; an event of a kind this build has
 * no word for is *by an event*.
 * @param {{started_by?: {by: string, event?: string, detail?: string | null} | null}} summary
 */
export function startedBy(summary) {
  const s = summary?.started_by;
  if (s?.by === "test") return t("workflow-runs-test-run");
  if (s?.by === "event") {
    const words = typeof s.event === "string" && Object.prototype.hasOwnProperty.call(EVENT_WORDS, s.event) ? EVENT_WORDS[s.event] : null;
    return words ? words(s.detail || null) : t("workflow-runs-started-by-event");
  }
  return t("workflow-runs-started-by-hand");
}

/**
 * The verbs a run admits here: *Stop* while a run of the workspace goes,
 * *Restart* for any run of the workspace (a finished one starts again with
 * its inputs) unless its workflow is put away — an archived workflow starts
 * no run, so the verb could only be refused — *Open* always. A goal's run
 * is stopped and restarted from its goal, so it offers neither.
 * @param {{scope: string, status: string}} summary
 * @param {{archived?: boolean}} [workflow] how the run's workflow stands now
 */
export function runRowVerbs(summary, workflow = {}) {
  const workspace = summary.scope === "workspace";
  return { stop: workspace && isLive(summary.status), restart: workspace && !workflow.archived, open: true };
}

/**
 * The Runs pane's rows: each summary with its title, its words, who started
 * it and its verbs, in `orderRuns` order.
 * @param {readonly object[] | null | undefined} runs
 * @param {{archived?: boolean}} [workflow] how the workflow stands now
 */
export function runsPaneRows(runs, workflow = {}) {
  return orderRuns(runs).map((r) => ({
    id: r.id,
    title: runTitle(r),
    status: r.status,
    words: runWords(r),
    startedBy: startedBy(r),
    verbs: runRowVerbs(r, workflow),
    live: isLive(r.status),
  }));
}

/** How many rows the Runs pane draws at first, and how many more each *Show older* adds. */
export const RUNS_SHOWN = 50;

/**
 * The rows the pane draws out of all it read — a workflow that listens
 * keeps every run it ever made, and a list is never drawn without a bound:
 * the first `shown` rows of the order, and never fewer than the runs still
 * going, which lead it. `hidden` is what is left out and `more` what the
 * next *Show older* brings.
 * @template T
 * @param {readonly (T & {live: boolean})[]} rows in `runsPaneRows` order
 * @param {number} [shown] how many the person asked to see
 */
export function paneWindow(rows, shown = RUNS_SHOWN) {
  const asked = Number.isFinite(shown) && shown > 0 ? Math.floor(shown) : RUNS_SHOWN;
  const limit = Math.max(asked, rows.filter((r) => r.live).length);
  const hidden = Math.max(0, rows.length - limit);
  return { rows: rows.slice(0, limit), hidden, more: Math.min(hidden, RUNS_SHOWN) };
}

/** The engine facts that move a run's row: a run started, finished or cancelled, one of its steps moved, or a boundary event fired on one. */
const RUN_FACTS = new Set(["run_started", "run_finished", "run_cancelled", "step_changed", "boundary_fired"]);

/**
 * Whether an engine event is about a run of this workflow — what the Runs
 * pane and a run's page refresh on. The envelope names the workflow of the
 * run behind a payload (`EngineEvent.workflow`).
 * @param {{workflow?: string | null, payload?: {type?: string}}} event
 * @param {string} workflow
 */
export function movesRunsOf(event, workflow) {
  return event?.workflow === workflow && RUN_FACTS.has(event?.payload?.type ?? "");
}

/** What a run owes a person moves with its gates: one opened on it, one decided, a question a session asked. */
const RUN_PAGE_FACTS = new Set([...RUN_FACTS, "gate_opened", "gate_decided", "question_asked"]);

/**
 * Whether an engine event moves the page of this run: a fact about the run
 * itself — the envelope names it (`EngineEvent.run`) — among the run facts
 * and the gates that feed *Your move*; or its workflow deleted, which takes
 * its runs with it, so the page reads again and finds the run gone.
 * Another run of the workflow is its own page's.
 * @param {{run?: string | null, payload?: {type?: string, workflow?: string}} | null | undefined} event
 * @param {string | null | undefined} run the run the page shows
 * @param {string | null | undefined} workflow its workflow, once read
 */
export function movesRun(event, run, workflow) {
  const type = event?.payload?.type ?? "";
  if (type === "workflow_deleted") return !!workflow && event.payload.workflow === workflow;
  return !!run && event?.run === run && RUN_PAGE_FACTS.has(type);
}

/** The facts that move how many runs of a workflow go: one started, finished or cancelled. */
const COUNT_FACTS = new Set(["run_started", "run_finished", "run_cancelled"]);

/**
 * Whether an engine event moves a workflow's row — its runs in the
 * workspace that go (`runs.live`, the verbs *Stop every run* and *Restart
 * every run*) and the goals in motion on it (`used_by[].live`, the
 * designer's freeze): a run of it, a goal's or the workspace's, that
 * started or ended. A step inside a run moves no count.
 * @param {{workflow?: string | null, payload?: {type?: string}} | null | undefined} event
 * @param {string} workflow
 */
export function movesRunCount(event, workflow) {
  return !!workflow && event?.workflow === workflow && COUNT_FACTS.has(event?.payload?.type ?? "");
}

/**
 * What a run's page decides before it draws (`GET /runs/{rid}`): the
 * header's title, status words, holder, who started it and the revision it
 * froze; the verbs it admits; whether it finished and the steps live now —
 * the rows that stand open on arrival; the line under the rows once it
 * ended — a cancel's cause, or the outcome with the step it failed at and
 * why; and, for a goal's run, where the page hands it on: the goal's
 * Workflow tab, on this run. `null` while nothing is read.
 * @param {{run: object, summary: object, holder: string, needs_actions?: readonly object[]} | null | undefined} view
 */
export function runPageFacts(view) {
  if (!view?.run || !view.summary) return null;
  const { run, summary } = view;
  const words = runWords(summary);
  const { finished } = runStanding(run);
  const goal = summary.goal ?? null;
  return {
    title: runTitle(summary),
    words,
    holder: HOLDER_LABEL[view.holder] ?? String(view.holder ?? ""),
    startedBy: startedBy(summary),
    revision: t("workflow-goal-workflow-tab-rev", { revision: summary.revision }),
    verbs: runRowVerbs(summary),
    finished,
    live: currentSteps(run),
    ended: !finished ? null : run.cancelled ? t("goal-progress-tab-run", { cancelled: words.word }) : finishedWords(run),
    handsTo: goal ? { route: { name: "goal", id: goal }, search: { tab: "workflow", run: run.id } } : null,
  };
}

/**
 * One verb at a time on a run. *Stop* and *Restart* are writes, and a
 * restart pressed twice would start two runs: a press is **taken** only
 * while the run has no verb on its way, and the run takes one again once
 * the node answered — done or refused (`settled`). The sets are never
 * written to: each answer is a new one, or the one handed in.
 * @param {ReadonlySet<string>} flying the runs with a verb on its way
 * @param {string} run
 * @returns {Set<string> | null} the set with this run in it, or `null` when its verb is already on its way
 */
export function taken(flying, run) {
  if (flying.has(run)) return null;
  return new Set([...flying, run]);
}

/** Whether a verb of this run is on its way: its buttons wait. @param {ReadonlySet<string>} flying @param {string} run */
export function inFlight(flying, run) {
  return flying.has(run);
}

/**
 * The node answered the run's verb: the run takes one again.
 * @param {ReadonlySet<string>} flying
 * @param {string} run
 * @returns {ReadonlySet<string>} the same set when the run had nothing on its way
 */
export function settled(flying, run) {
  if (!flying.has(run)) return flying;
  const next = new Set(flying);
  next.delete(run);
  return next;
}

/**
 * How a read of runs stands, and the one line it says. A re-read that
 * failed keeps the last answer on screen (`stale`) and says why beside it —
 * a run does not leave the page because the node was away for a moment;
 * with nothing read yet the failure is the note (`failed`); a run the node
 * says is gone shows the note alone (`gone`), never what the window had
 * kept of it.
 * @param {{data?: unknown, loading?: boolean, error?: string | null, missing?: boolean}} read
 * @param {string} what what is read, as a sentence names it: *the run*, *the runs*
 * @returns {{standing: "reading" | "ready" | "stale" | "failed" | "gone", line: string | null}}
 */
export function readStanding(read, what) {
  const has = read.data !== null && read.data !== undefined;
  if (read.missing) return { standing: "gone", line: read.error ?? null };
  if (read.error && !has) return { standing: "failed", line: read.error };
  if (read.error) return { standing: "stale", line: readWords({ what, data: read.data, error: read.error })?.text ?? read.error };
  return { standing: has ? "ready" : "reading", line: null };
}

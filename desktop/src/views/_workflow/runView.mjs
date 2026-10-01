/**
 * How a run reads on the canvas: the tone of each step, the tone of each
 * edge, the actions a person has on a step, the boundary events that fired,
 * and the run's progress in one number — with no React in it.
 *
 * The tones are **theme roles**, never colours: the canvas paints a step's
 * ring with `var(--color-<role>)`, so a run looks right in every theme and
 * the designer never names a hex value. `STEP_TONE_TOKENS` is the whole
 * vocabulary, and `runView.test.mjs` checks it against the role contract.
 */

import { t } from "../../i18n/l10n.mjs";
import { cancelWords, runIndex, runStatus, runWords } from "../_goal/runControl.mjs";

/** A step's state → the role its ring wears. A divert is a detour, not a failure: it wears the warning. */
export const STEP_TONE_TOKENS = {
  pending: "text-dim",
  running: "accent",
  waiting: "warn",
  done: "ok",
  skipped: "text-dim",
  failed: "danger",
  cancelled: "text-dim",
  diverted: "warn",
};

/** A step's state in one word, as the legend and a chip say it — every state the core names has one. */
const STATE_WORDS = Object.freeze({
  pending: () => t("workflow-run-view-pending"),
  running: () => t("workflow-run-view-running"),
  waiting: () => t("workflow-run-view-state-waiting"),
  done: () => t("workflow-run-view-done"),
  skipped: () => t("workflow-run-view-skipped"),
  failed: () => t("workflow-run-view-failed"),
  cancelled: () => t("workflow-run-view-cancelled"),
  diverted: () => t("workflow-run-view-state-diverted"),
});

/** The states the legend above a run's canvas names, in the order a run meets them. */
export const LEGEND_STATES = Object.freeze(["running", "waiting", "done", "diverted", "failed", "skipped"]);

/** The word for a step's state; a state this build has no word for is said as the node said it. @param {string} state */
export function stepStateWord(state) {
  return Object.prototype.hasOwnProperty.call(STATE_WORDS, state) ? STATE_WORDS[state]() : String(state);
}

/** The kinds that are no work of their own: a start is done the moment the run enters it, a parallel the moment it fans out. */
const NOT_WORK = new Set(["start", "parallel"]);

/** The record's state name, `pending` when the run has none for the step. */
function stepStateOf(run, id) {
  return run?.steps?.[id]?.state?.state ?? "pending";
}

export function stepTone(run, id) {
  return STEP_TONE_TOKENS[stepStateOf(run, id)] ?? "text-dim";
}

/** The branches a finished step chose — one, several for `pick = every`, none for a step that is no gateway. */
export function branchesChosen(state) {
  if (!state || state.state !== "done") return [];
  return Array.isArray(state.branches) ? state.branches : [];
}

/** One line on a step's record: what it is doing, or how it ended — every word the catalog's. */
export function stepLabel(run, id) {
  const rec = run?.steps?.[id];
  const state = stepStateOf(run, id);
  switch (state) {
    case "running":
      return rec?.work_item ? t("workflow-run-view-running-item", { work_item: String(rec.work_item).slice(-6) }) : t("workflow-run-view-running");
    case "waiting":
      return t("workflow-run-view-waiting");
    case "done": {
      const branches = branchesChosen(rec?.state);
      // A loop step mid-way says where it stands: `each 2 of 5`, `loop 3`.
      const cursor = rec?.cursor;
      if (cursor && branches.length === 1 && branches[0] !== "done") {
        const count = Array.isArray(cursor.items) ? cursor.items.length : null;
        return count === null ? t("workflow-run-view-loop-place", { branch: branches[0], index: cursor.index }) : t("workflow-run-view-words", { branch: branches[0], index: cursor.index, count });
      }
      return branches.length > 0 ? t("workflow-run-view-done-chose", { branches: branches.join(", ") }) : t("workflow-run-view-done");
    }
    case "diverted":
      return t("workflow-run-view-diverted", { by: rec?.state?.by ?? "" });
    case "failed":
      return rec?.error ? t("workflow-run-view-failed-because", { error: rec.error }) : t("workflow-run-view-failed");
    case "skipped":
      return t("workflow-run-view-skipped");
    case "cancelled":
      return t("workflow-run-view-cancelled");
    default:
      return rec?.visits ? t("workflow-run-view-pending-visited", { visits: rec.visits }) : t("workflow-run-view-pending");
  }
}

/**
 * Progress as a fraction: the steps of work that reached an end over the
 * steps of work that exist — a `start` and a `parallel` are no work of their
 * own. A skipped step counts as reached — an untaken branch is not work
 * left — so does a diverted one, and a cancelled run counts what it got to.
 */
export function progress(run) {
  const steps = (run?.workflow?.steps ?? []).filter((s) => !NOT_WORK.has(s.kind));
  if (steps.length === 0) return 0;
  const reached = steps.filter((s) => ["done", "skipped", "failed", "cancelled", "diverted"].includes(stepStateOf(run, s.id)));
  return reached.length / steps.length;
}

/** The steps that are live right now — running or waiting. */
export function currentSteps(run) {
  return (run?.workflow?.steps ?? []).filter((s) => ["running", "waiting"].includes(stepStateOf(run, s.id))).map((s) => s.id);
}

/**
 * The boundary events that fired during the step's current visit, by name —
 * what lights a chip on the run's canvas.
 */
export function firedBoundaries(run, id) {
  const fired = run?.steps?.[id]?.fired ?? {};
  return new Set(Object.entries(fired).filter(([, f]) => (f?.count ?? 0) > 0).map(([name]) => name));
}

/**
 * What a person can do to one step, from its kind and state.
 *
 * - `answer`: a `human` step that is waiting.
 * - `done`: the same step, done by hand.
 * - `decide`: an `approval` step that is waiting — through the inbox's gate.
 * - `release`: a `wait` step holding for a person.
 * - `open`: an `agent` step with a work item to look at.
 */
export function stepActions(step, record) {
  const state = record?.state?.state ?? "pending";
  const out = [];
  if (state === "waiting") {
    if (step.kind === "human") out.push("answer", "done");
    if (step.kind === "approval") out.push("decide");
    if (step.kind === "wait" && step.until?.until === "release") out.push("release");
  }
  if (step.kind === "agent" && record?.work_item) out.push("open");
  return out;
}

/** How a step's failure is passed on, from the run's frozen workflow: `fail` when it says nothing. */
function onFailOf(run, id) {
  const step = (run?.workflow?.steps ?? []).find((s) => s.id === id);
  return step?.on_fail ?? { on_fail: "fail" };
}

/**
 * An edge's tone: `taken` when its source finished and the flow was the one
 * followed, `skipped` when the source finished another way, `default` while
 * it is still open — the core's `WorkflowRun::edge`, restated. A step's own
 * flow is taken when the step finished and chose no branch (an unlabelled
 * flow) or chose its label (a gateway may choose several); a boundary's
 * path is taken only when that boundary diverted the step — a diverted step
 * takes nothing else. A step that **failed** takes what its `on_fail` says:
 * passed over (`skip`), its unlabelled flows — the run went on along them;
 * routed (`then`), the route, and a flow of its own that leads to the same
 * step; `fail`, nothing. An `on_fail` edge is taken when its source failed;
 * a diverted step never fails.
 */
export function edgeTone(run, edge) {
  const rec = run?.steps?.[edge.from];
  const state = rec?.state?.state ?? "pending";
  if (edge.kind === "on_fail") return state === "failed" ? "taken" : state === "done" || state === "diverted" ? "skipped" : "default";
  if (state === "diverted") return edge.branch !== null && edge.branch === rec.state.by ? "taken" : "skipped";
  if (state === "done") {
    if (edge.kind === "boundary") return "skipped";
    const branches = branchesChosen(rec.state);
    return edge.branch === null ? (branches.length === 0 ? "taken" : "skipped") : branches.includes(edge.branch) ? "taken" : "skipped";
  }
  if (state === "skipped" || state === "cancelled") return "skipped";
  if (state === "failed") {
    const onFail = onFailOf(run, edge.from);
    if (onFail.on_fail === "skip") return edge.branch === null ? "taken" : "skipped";
    if (onFail.on_fail === "then") return edge.to === onFail.step ? "taken" : "skipped";
    return "skipped";
  }
  return "default";
}

/**
 * The step a failed run failed at — the newest record in `failed` — with its
 * error, for the sentence a finished run's tab reads. `null` for a run that
 * did not fail, or whose failure left no step (an unmerged shape).
 * @returns {{id: string, name: string, error: string | null} | null}
 */
export function failedStep(run) {
  if (!run || run.outcome !== "failed") return null;
  let best = null;
  for (const [id, record] of Object.entries(run.steps ?? {})) {
    if (record?.state?.state !== "failed") continue;
    if (!best || (record.seq ?? 0) > best.seq) best = { id, seq: record.seq ?? 0, error: record.error ?? null };
  }
  if (!best) return null;
  const name = (run.workflow?.steps ?? []).find((s) => s.id === best.id)?.name ?? best.id;
  return { id: best.id, name, error: best.error };
}

/**
 * The strip above a run's canvas, as facts: the run's status read off the
 * run itself (`runControl.runStatus` — the core's projection, so the strip,
 * the header and the Runs list say one word), the words and the tone that
 * status wears, its number among the goal's runs (`null` when the list does
 * not hold it), the revision it froze, how far it got, what is live, and
 * the two moments that matter — when it began (started, or queued while it
 * waits its turn) and how it ended (finished, or the cause of its cancel).
 * A queued run's place is the list's; nothing else is.
 * @param {object} run the `WorkflowRun` the canvas wears
 * @param {readonly object[] | null | undefined} runs the goal's run summaries
 */
export function overlayFacts(run, runs) {
  const listed = (runs ?? []).find((r) => r.id === run.id) ?? null;
  const status = runStatus(run);
  const words = runWords({
    status,
    cause: run.cancelled ?? null,
    position: listed?.position ?? null,
    queued_at: run.queued_at ?? null,
    started_at: run.started_at ?? null,
    finished_at: run.finished_at ?? null,
  });
  const began = run.started_at != null ? { word: t("goal-run-control-at-started"), at: run.started_at } : { word: t("goal-run-control-at-queued"), at: run.queued_at ?? null };
  const ended = run.finished_at ? { word: run.cancelled ? cancelWords(run.cancelled) : t("goal-run-control-at-finished"), at: run.finished_at } : null;
  return {
    status,
    word: words.word,
    tone: words.tone,
    queued: status === "queued",
    index: runIndex(runs, run.id),
    revision: run.workflow?.revision ?? null,
    percent: Math.round(progress(run) * 100),
    live: currentSteps(run),
    began,
    ended,
  };
}

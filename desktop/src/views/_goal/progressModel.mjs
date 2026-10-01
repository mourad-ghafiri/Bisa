/**
 * The run read downward: one row per step of work of the frozen workflow,
 * in definition order, with what a person needs to follow it — state, who
 * holds it, when it moved, what it produced — and the verbs the step admits
 * right now (`stepActions`, the same rule the canvas uses). A `start` and a
 * `parallel` are no work of their own — one is done the moment the run
 * enters it, the other the moment it fans out — so they are not rows. The
 * holder word per step mirrors the core's `waiting_holder`, checked in the
 * test against the Rust source.
 */
import { stepActions } from "../_workflow/runView.mjs";
import { duration } from "../../i18n/format.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The kinds that route or begin and hold nobody: never a row of work. */
const NOT_WORK = new Set(["start", "parallel"]);

/**
 * Who a live step waits on: the core's ladder, step-sized. `null` when the
 * step is not live. A wait held by the world — a delay, a moment, a
 * schedule, a signal, a message, a project's change, a run's end, a platform
 * event — waits on the world; one a person releases, on you.
 */
export function stepHolder(kind, state, until = null) {
  if (state === "running") return "agents";
  if (state !== "waiting") return null;
  if (kind === "human" || kind === "approval") return "you";
  if (kind === "wait") return until === "release" ? "you" : "world";
  if (kind === "spawn") return "world";
  return "agents";
}

function rowOf(step, record, current, now) {
  const state = record?.state ?? { state: "pending" };
  const started = record?.started_at ?? null;
  const finished = record?.finished_at ?? null;
  const live = state.state === "running" || state.state === "waiting";
  const until = step.kind === "wait" ? (step.until?.until ?? null) : null;
  const secs = started === null ? null : finished !== null ? Math.max(0, finished - started) : live ? Math.max(0, now - started) : null;
  return {
    id: step.id,
    name: step.name || step.id,
    kind: step.kind,
    state,
    holder: stepHolder(step.kind, state.state, until),
    startedAt: started,
    finishedAt: finished,
    durationSecs: secs,
    // How long, in the platform's one way of saying a span; nothing for a step that never started.
    duration: secs === null ? null : duration(secs),
    workItem: record?.work_item ?? null,
    output: record?.output ?? null,
    error: record?.error ?? null,
    answer: record?.answer ?? null,
    visits: record?.visits ?? 0,
    actions: stepActions(stepShape(step), record ?? null),
    current,
  };
}

/** `stepActions` reads the whole step; a row carries the kind tag only. */
function stepShape(step) {
  return step.shape ?? step;
}

/**
 * The rows. With a run, its frozen workflow in definition order; without one,
 * the strip's ghosted definition as pending rows — a chosen workflow that has
 * not started reads as the plan it is.
 * @param {object|null} run   the `WorkflowRun` or null
 * @param {object} strip      the node's `RunStrip`
 * @param {number} [now]      unix seconds, for a live step's elapsed time
 */
export function progressRows(run, strip, now = Math.floor(Date.now() / 1000)) {
  const current = new Set(strip?.current ?? []);
  if (run?.workflow?.steps) {
    // The wire flattens a step's kind: `kind` is the word, the kind's fields
    // sit beside it (`until` on a wait step), and `stepActions` reads the step as is.
    return run.workflow.steps
      .filter((s) => !NOT_WORK.has(s.kind))
      .map((s) =>
        rowOf(
          { id: s.id, name: s.name, kind: s.kind, until: s.kind === "wait" ? s.until : undefined, shape: s },
          run.steps?.[s.id] ?? null,
          current.has(s.id),
          now,
        ),
      );
  }
  return (strip?.steps ?? []).filter((s) => !NOT_WORK.has(s.kind)).map((s) => rowOf({ id: s.id, name: s.name, kind: s.kind, shape: null }, null, current.has(s.id), now));
}

/**
 * The steps of a run that are live — running or waiting — in definition
 * order: the ones a person may act on without opening the goal.
 * @param {{workflow?: {steps?: {id: string}[]}, steps?: Record<string, {state?: {state: string}}>} | null | undefined} run
 */
export function liveSteps(run) {
  return (run?.workflow?.steps ?? []).filter((s) => {
    const state = run.steps?.[s.id]?.state?.state;
    return state === "running" || state === "waiting";
  });
}

/**
 * The finished banner's sentence: what the run came to — and, failed, at
 * which step and why, so the reason is read where the run is, not under a
 * collapsed row.
 * @param {{outcome?: string | null, workflow?: {steps?: {id: string, name: string}[]}, steps?: Record<string, {state?: {state: string}, seq?: number, error?: string | null}>}} run
 */
export function finishedWords(run) {
  if (run?.outcome !== "failed") return t("goal-progress-run-finished", { outcome: run?.outcome ?? "done" });
  const failed = Object.entries(run.steps ?? {})
    .filter(([, r]) => r?.state?.state === "failed")
    .sort((a, b) => (b[1].seq ?? 0) - (a[1].seq ?? 0))[0];
  if (!failed) return t("goal-progress-run-finished-failed");
  const [id, record] = failed;
  const name = run.workflow?.steps?.find((s) => s.id === id)?.name || id;
  return t("goal-progress-run-failed", { name, error: record.error, flag: (record.error) ? "yes" : "no" });
}

/** The word that says a person closed a step's row by hand, beside the step's id for one opened by hand. */
const closedWord = (id) => `!${id}`;

/**
 * Whether a step's row stands open: what a person said of it by hand, else
 * open while it is live. `said` holds a step's id for a row opened by hand
 * and `!<id>` for one closed by hand — the words the screen keeps.
 * @param {ReadonlySet<string>} said
 * @param {string} id the step
 * @param {boolean} live the step is live, or was when the page opened
 */
export function rowOpen(said, id, live) {
  if (said.has(id)) return true;
  return live && !said.has(closedWord(id));
}

/**
 * What a person has said after pressing a step's row: a row standing open
 * is closed, one standing closed is opened. A word is kept only where it
 * says other than the row does on its own — a live row closed, a quiet row
 * opened — so a step that goes quiet closes by itself unless it was opened
 * by hand. A new set either way.
 * @param {ReadonlySet<string>} said
 * @param {string} id the step
 * @param {boolean} live
 * @returns {Set<string>}
 */
export function rowPressed(said, id, live) {
  const open = rowOpen(said, id, live);
  const next = new Set(said);
  next.delete(id);
  next.delete(closedWord(id));
  if (open && live) next.add(closedWord(id));
  if (!open && !live) next.add(id);
  return next;
}

/** "3 of 7 steps", from the strip's reached/total. */
export function progressCount(strip) {
  const reached = strip?.reached ?? 0;
  const total = strip?.total ?? 0;
  return { reached, total, label: total === 0 ? t("goal-progress-no-steps-yet") : t("goal-progress-steps", { reached, total }) };
}

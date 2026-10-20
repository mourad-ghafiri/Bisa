/**
 * What the goal page decides before it draws: what its header names the goal
 * by, where the run stands, whether a design waits to be adopted, whether
 * the design is the goal's own, and how the design begins — on events, which
 * the goal listens for while it is open, or by hand. `GoalDetail.tsx` paints
 * these; the run's verbs are `runControl`'s and the Workflow Agent's state
 * `designStatus`'s.
 */

import { designs, modeOf } from "./goalMode.mjs";
import { listens as designListens, manualEntry as designEntry } from "../_workflow/forms/startForm.mjs";
import { ADOPT_SUBJECT, namesAdoption } from "./proposalRouting.mjs";
import { t } from "../../i18n/l10n.mjs";

export { ADOPT_SUBJECT };

/**
 * What the header names the goal by — its words, never its id. A titled goal
 * is its title with the statement under it; an untitled one is the statement
 * itself, the sentence the person captured, with nothing under it (the same
 * words twice say less than once); a statement that only repeats the title is
 * not said again either. A goal with neither word — the node refuses one, so
 * this is the shape's fallback alone — is *Untitled goal*.
 * @param {{title?: string | null, statement?: string | null} | null | undefined} goal
 * @returns {{title: string, subtitle: string | null}}
 */
/**
 * The frames that change what the goal page shows — its run, its design, its
 * listening, its gates, its attachments, its own record — and are read
 * again when they arrive. `guided` is among them only when its status
 * moves the page: a proposal, a failure, a stall, the agent off. A tick of
 * the design in progress (`scheduled`, `working`, `asking`) is the
 * Designing card's own, applied to its state; reading the whole goal on
 * every one was what kept the Workflow tab re-rendering.
 */
export const PAGE_FACTS = Object.freeze([
  "run_started",
  "run_queued",
  "run_finished",
  "run_cancelled",
  "step_changed",
  "goal_closed",
  "workflow_proposed",
  "workflow_changed",
  "workflow_archived",
  "goal_archived",
  "goal_updated",
  "gate_opened",
  "gate_decided",
  "question_asked",
  "result_accepted",
  "execution_ended",
  "listening_changed",
  "listener_failed",
  "attachment_changed",
  "document_added",
  "project_created",
]);

/** The design statuses that are a tick of work in progress, not a move of the page. */
const GUIDED_TICKS = new Set(["scheduled", "working", "asking"]);

/**
 * Whether the page reads the goal again on this frame (`PAGE_FACTS`).
 * @param {{type?: unknown, status?: unknown} | null | undefined} payload
 */
export function wantsReload(payload) {
  if (!payload || typeof payload.type !== "string") return false;
  if (payload.type === "guided") return !GUIDED_TICKS.has(payload.status);
  return PAGE_FACTS.includes(payload.type);
}

export function headerWords(goal) {
  const title = String(goal?.title ?? "").trim();
  const statement = String(goal?.statement ?? "").trim();
  if (title) return { title, subtitle: statement && statement !== title ? statement : null };
  return { title: statement || t("goal-goal-header-untitled-goal"), subtitle: null };
}

/** The open gate that asks whether the designed workflow is adopted, or none. */
export function adoptGateOf(pendingGates) {
  return (Array.isArray(pendingGates) ? pendingGates : []).find((g) => g?.gate === "approval" && namesAdoption(g?.subject));
}

/**
 * Where the run stands: `finished` once it has an outcome or was cancelled,
 * `unfinished` while there is one that has neither; with no run, neither.
 */
export function runStanding(run) {
  const finished = !!run && (run.outcome != null || !!run.cancelled);
  return { finished, unfinished: !!run && !finished };
}

/**
 * Whether a designed workflow waits on the person: the goal's mode is one
 * the Workflow Agent designs in, a workflow is set, no run has started, and
 * the adoption's gate is open — all four, or the Progress tab would offer a
 * plan nobody proposed, or hide one somebody did.
 */
export function isProposed(goal, pendingGates) {
  return designs(modeOf(goal)) && !!goal?.workflow && !goal?.run && !!adoptGateOf(pendingGates);
}

/**
 * The design of the goal's own — what *Promote to library* copies out — or
 * null: a workflow picked from the library is already there.
 */
export function ownDesignOf(startable) {
  return startable && startable.origin?.origin === "goal" ? startable : null;
}

/**
 * The goal's listening and its design's ways in: its `Listening` (armed or
 * paused, `null` when it does not listen), whether the design begins on
 * events — started by listening — and its start by hand, where *Run now*
 * begins (`null` when only events begin it).
 */
export function listeningFacts(goal, startable) {
  return { listening: goal?.listening ?? null, listens: designListens(startable), manualEntry: designEntry(startable)?.id ?? null };
}

/**
 * The goals that may take a closed goal's place: every goal that is open
 * and not put away, but itself — each by its title, or its statement when
 * it has none.
 * @param {readonly {id: string, title?: string | null, statement?: string, status?: string, archived?: unknown}[] | null | undefined} goals
 * @param {string} goal the goal being closed
 * @returns {{id: string, label: string}[]}
 */
export function replacements(goals, goal) {
  return (Array.isArray(goals) ? goals : [])
    .filter((g) => g && g.id !== goal && g.status !== "closed" && !g.archived)
    .map((g) => ({ id: g.id, label: (g.title ?? "").trim() || String(g.statement ?? "").trim() || g.id }));
}

/**
 * The goal a close names as its replacement, when it names one that may be:
 * a real id of `replacements`, never the goal itself, never an empty string.
 */
function replacedBy(form, goal) {
  const by = typeof form?.replacedBy === "string" ? form.replacedBy.trim() : "";
  return by && by !== goal ? by : null;
}

/**
 * The body of `POST /goals/{id}/close` (`CloseBody`): the person's word on
 * why, or the goal that takes this one's place — a supersession is sent as
 * `superseded_by`, and never beside a rationale: a goal replaced is
 * *superseded*, and the node keeps no reason beside the goal that replaces
 * it. What was not given is absent — a reference is a real id or absent,
 * never an empty string.
 * @param {{rationale?: string | null, replacedBy?: string | null} | null | undefined} form
 * @param {string} goal the goal being closed
 * @returns {{rationale?: string, superseded_by?: string}}
 */
export function closeBody(form, goal) {
  const by = replacedBy(form, goal);
  if (by) return { superseded_by: by };
  const why = typeof form?.rationale === "string" ? form.rationale.trim() : "";
  return why ? { rationale: why } : {};
}

/**
 * What the close dialog says will be recorded, and what the toast says was:
 * *abandoned*, or *superseded by* the goal that replaces it, by name.
 * @param {{rationale?: string | null, replacedBy?: string | null} | null | undefined} form
 * @param {readonly {id: string, title?: string | null, statement?: string}[] | null | undefined} goals
 * @param {string} goal the goal being closed
 * @returns {{records: string, closed: string}}
 */
export function closeWords(form, goals, goal) {
  const by = replacedBy(form, goal);
  if (!by) return { records: t("screens-goal-detail-close-records-abandoned"), closed: t("screens-goal-detail-closed") };
  const name = replacements(goals, goal).find((g) => g.id === by)?.label ?? by;
  return { records: t("screens-goal-detail-close-records-superseded", { goal: name }), closed: t("screens-goal-detail-closed-superseded", { goal: name }) };
}

/** Everything above, for the page: one reading of the goal's data. */
export function pageFacts({ goal, run, pendingGates, startable }) {
  return {
    closed: !!goal?.closed,
    ...runStanding(run),
    adoptGate: adoptGateOf(pendingGates),
    proposed: isProposed(goal, pendingGates),
    ownDesign: ownDesignOf(startable),
    ...listeningFacts(goal, startable),
  };
}

/**
 * The goal card's facts: what the Goals list shows about one goal
 * beyond the strip — the statement as a second line, the status glyph's
 * tone, the projects it carries, how many things wait on you, who is
 * assigned, and the strip folded to one line when a workflow is long. The
 * card (`GoalCard.tsx`) paints; the rules are here, with tests.
 */

import { t } from "../../i18n/l10n.mjs";

/** The theme role each goal status wears. */
export const STATUS_TONE = Object.freeze({
  draft: "text-dim",
  running: "accent",
  waiting: "warn",
  done: "ok",
  failed: "danger",
  closed: "text-dim",
});

/** The tone of a status, `text-dim` for a word the list does not know. */
export function statusTone(status) {
  return STATUS_TONE[status] ?? "text-dim";
}

/** A goal's status as a person reads it, under the glyph. */
const STATUS_WORD = Object.freeze({
  draft: t("goals-goal-card-status-draft"),
  running: t("goals-goal-card-status-running"),
  waiting: t("goals-goal-card-status-waiting"),
  done: t("goals-goal-card-status-done"),
  failed: t("goals-goal-card-status-failed"),
  closed: t("goals-goal-card-status-closed"),
});

/** The word of a status; one this build has no word for is said as the node said it. */
export function statusWord(status) {
  return STATUS_WORD[status] ?? String(status ?? ""); // for the machine
}

/**
 * The second line: the statement, when the title is not already it. A goal
 * with no title shows its statement as the title and nothing beneath.
 */
export function secondLine(row) {
  if (!row?.title) return null;
  const s = String(row.statement ?? "").trim();
  return s && s !== row.title ? s : null;
}

/**
 * The strip folded to at most `max` chips: the first ones, the current ones
 * kept even when they fall past the fold, and a count of what is hidden. A
 * run of twenty steps stays one line.
 * @returns {{shown: object[], hidden: number}}
 */
export function compactChips(steps, current, max = 12) {
  const all = steps ?? [];
  if (all.length <= max) return { shown: all, hidden: 0 };
  const live = new Set(current ?? []);
  const shown = [];
  const keepCurrent = all.filter((s) => live.has(s.id));
  const room = Math.max(1, max - keepCurrent.length);
  for (const s of all) {
    if (shown.length >= room) break;
    if (live.has(s.id)) continue;
    shown.push(s);
  }
  const merged = all.filter((s) => shown.includes(s) || live.has(s.id));
  return { shown: merged, hidden: all.length - merged.length };
}

/** The projects attached to a goal, from the workspace's one project list. */
export function projectsOf(goalId, projects) {
  return (projects ?? []).filter((p) => (p.goals ?? []).includes(goalId));
}

/** How many things wait on a person in this goal, from the inbox rows. */
export function needsYouCount(goalId, inbox) {
  const row = (inbox ?? []).find((r) => r.key === goalId);
  return row?.needs_action?.length ?? 0;
}

/**
 * Who is assigned, as short words: an agent by id, a team by id, a person by
 * the head of their key. At most `max`, then how many more.
 */
export function assigneeSummary(assignees, max = 3) {
  const words = (assignees ?? []).map((a) => {
    if (a.agent) return { kind: "agent", word: a.agent };
    if (a.team) return { kind: "team", word: a.team };
    if (a.human) return { kind: "human", word: String(a.human).slice(0, 8) };
    return { kind: "unknown", word: "?" };
  });
  return { shown: words.slice(0, max), more: Math.max(0, words.length - max) };
}

/** The workflow's name for the card, or nothing when the goal has none yet. */
export function workflowWord(strip) {
  const name = strip?.workflow_name;
  return name ? String(name) : null;
}

/**
 * The run verbs a list row offers, from the row alone — no fetch per goal:
 * `run_status` says whether a run is live, `queued` how many wait, the
 * holder whether the Workflow Agent is designing. A first start is the goal
 * page's (an adoption may be owed); the row offers *New run…* once the
 * goal ran, *Restart* likewise, *Stop* whenever a run is live or queued.
 * The same shape `runControl.mjs`'s `runVerbs` answers.
 */
export function rowVerbs(row) {
  const none = { start: null, stop: null, restart: null };
  if (!row || row.status === "closed" || row.closed) return none;
  const live = row.run_status === "running" || row.run_status === "waiting";
  const queued = row.queued ?? 0;
  const designing = row.holder === "design";
  const ran = !!row.run;
  const startable = !!row.workflow && ran;
  // A listening goal's run by hand begins at its design's start by hand,
  // which only the goal page knows: the row offers no new run of it.
  const listening = !!row.listening;
  return {
    start: startable && !designing && !listening ? { label: t("goals-goal-card-new-run"), queues: live, adopt: false } : null,
    stop: live || queued > 0 ? { label: t("goals-goal-card-stop"), live, queued } : null,
    restart: startable && !designing ? { label: t("goals-goal-card-restart") } : null,
  };
}

/**
 * The card's `⋮`, in the one shape the workflow card shares
 * (`workflowCardModel.cardMenu`): *Open* first; the run's verbs — the first
 * opening its group, *Stop* in danger; *Delete…* last, in danger, after a
 * separator. Every card has the first and the last: a goal with nothing to
 * run still opens and can go, and deleting from the list is the goal page's
 * retirement dialog — it says what runs and stops it first.
 * @param {{start: object | null, stop: object | null, restart: object | null} | null | undefined} verbs `rowVerbs`
 * @returns {{id: "open" | "start" | "restart" | "stop" | "delete", label: string, danger?: boolean, separatorBefore?: boolean}[]}
 */
export function cardMenu(verbs) {
  const items = [{ id: "open", label: t("goals-goal-card-open") }];
  const run = [];
  if (verbs?.start) run.push({ id: "start", label: verbs.start.label });
  if (verbs?.restart) run.push({ id: "restart", label: verbs.restart.label });
  if (verbs?.stop) run.push({ id: "stop", label: verbs.stop.label, danger: true });
  run.forEach((item, i) => items.push({ ...item, separatorBefore: i === 0 }));
  items.push({ id: "delete", label: t("goals-goal-card-delete"), danger: true, separatorBefore: true });
  return items;
}

/**
 * The chip a listening goal wears on its row: *listening* while its start
 * events are armed, *paused* once a failed run or a spent budget stopped it
 * hearing them. `null` for a goal that does not listen.
 * @param {{listening?: {paused?: unknown} | null} | null | undefined} row
 * @returns {{words: string, tone: "accent" | "warn", paused: boolean} | null}
 */
export function listeningChip(row) {
  const listening = row?.listening;
  if (!listening) return null;
  return listening.paused ? { words: t("goals-goal-card-paused"), tone: "warn", paused: true } : { words: t("goals-goal-card-listening"), tone: "accent", paused: false };
}

/** The chip for the runs waiting behind the live one, or nothing when none wait. */
export function queuedChip(row) {
  const n = row?.queued ?? 0;
  return n > 0 ? t("goals-goal-card-queued", { n }) : null;
}

/**
 * Whether the Workflow Agent is designing this goal, as the list knows it:
 * the node says who holds the goal (`holder`), and a goal held by the design
 * in a mode that designs is being designed. The goal's own page reads the
 * agent's phases (`designStatus.designInProgress`); a list row carries none.
 * @param {{holder?: string} | null | undefined} row
 * @param {boolean} designsInMode `goalMode.designs(modeOf(row))`
 */
export function designingRow(row, designsInMode) {
  return designsInMode === true && row?.holder === "design";
}


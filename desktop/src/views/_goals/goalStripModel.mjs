/**
 * The Goals screen's facts: who holds a goal, how its strip reads, which
 * rows a filter keeps, and the order the list scrolls in.
 *
 * There are no sections and no lifecycle buckets here, on purpose: a goal is
 * its running workflow, so the screen is one flat, live list of run strips,
 * narrowed by filters (who holds the ball, which workflow, tags, text) and
 * sorted by last activity. Putting a goal behind the wrong filter is a wrong
 * *fact*, which is why this is a model with tests and the view is paint.
 *
 * The vocabulary mirrors closed Rust sets — `Holder` (core/goal.rs),
 * `StepKind` (core/workflow.rs), `StepState` (core/run.rs) — and the test
 * reads those sources, so a new variant fails a test here rather than
 * rendering as nothing.
 */

import { t } from "../../i18n/l10n.mjs";

/** Holder words in the order the filter offers them. */
export const HOLDERS = ["you", "agents", "world", "finished", "design"];

/** What each holder reads as on a row. */
export const HOLDER_LABEL = {
  you: t("goals-goal-strip-move"),
  agents: t("goals-goal-strip-agents-working"),
  world: t("goals-goal-strip-waiting-world"),
  finished: t("goals-goal-strip-finished"),
  design: t("goals-goal-strip-being-designed"),
};

/**
 * What each holder reads as in the filter — the option of a list, so it
 * begins a sentence: *Your move*, *Agents working*. The row's chip keeps the
 * lowercase word (`HOLDER_LABEL`) because there it follows other words.
 */
export const HOLDER_FILTER_LABEL = {
  you: t("goals-goals-filters-holder-you"),
  agents: t("goals-goals-filters-holder-agents"),
  world: t("goals-goals-filters-holder-world"),
  finished: t("goals-goals-filters-holder-finished"),
  design: t("goals-goals-filters-holder-design"),
};

/**
 * Theme roles, not colours: you = the accent (it wants you — the one colour
 * the tokens file reserves for a summons), agents = text (work is happening,
 * and nothing is asked of you; the strip's working glyph says it is live),
 * world/design = dim, finished = ok — or danger when the outcome is failed,
 * which is a tone rule, not a grouping.
 */
export const HOLDER_TONE = {
  you: "accent",
  agents: "text",
  world: "text-dim",
  finished: "ok",
  design: "text-dim",
};

/** A noun per step kind for the pill and the chip a11y labels. */
export const KIND_LABEL = {
  start: t("goals-goal-strip-start-event"),
  emit: t("goals-goal-strip-signal-raised"),
  parallel: t("goals-goal-strip-parallel-paths"),
  agent: t("goals-goal-strip-agent-step"),
  human: t("goals-goal-strip-kind-question"),
  approval: t("goals-goal-strip-kind-approval"),
  check: t("goals-goal-strip-kind-check"),
  decide: t("goals-goal-strip-kind-decision"),
  judge: t("goals-goal-strip-kind-judgement"),
  if: t("goals-goal-strip-kind-condition"),
  switch: t("goals-goal-strip-kind-switch"),
  for_each: t("goals-goal-strip-loop-over-items"),
  while: t("goals-goal-strip-kind-loop"),
  connector: t("goals-goal-strip-connector-call"),
  wait: t("goals-goal-strip-kind-wait"),
  notify: t("goals-goal-strip-kind-notification"),
  spawn: t("goals-goal-strip-child-goal"),
  end: t("goals-goal-strip-kind-end"),
};

/** The tone a finished strip earns: ok, or danger when it failed. */
export function finishedTone(strip) {
  return strip?.outcome === "failed" ? "danger" : "ok";
}

/** Where a failed strip failed, in a sentence for the row — `null` while nothing failed. */
export function failureWords(strip) {
  const failure = strip?.failure;
  if (!failure) return null;
  return t("goals-goal-strip-failed", { step: failure.name || failure.step, error: failure.error, flag: (failure.error) ? "yes" : "no" });
}

/** The chips of a strip, with the current ones marked. */
export function chipsOf(strip) {
  const steps = strip?.steps ?? [];
  const current = new Set(strip?.current ?? []);
  return steps.map((s) => ({
    id: s.id,
    name: s.name || s.id,
    kind: s.kind,
    state: s.state,
    current: current.has(s.id),
    label: `${s.name || s.id}: ${s.state.state}`,
  }));
}

/** The step to name beside the strip: the first live one, else nothing. */
export function currentStepOf(strip) {
  const current = strip?.current ?? [];
  if (current.length === 0) return null;
  const step = (strip?.steps ?? []).find((s) => s.id === current[0]) ?? null;
  return step ? { ...step, more: current.length - 1 } : null;
}

/** The workflow facets the rows offer, `{name, count}` by name. */
export function workflowFacets(rows) {
  const counts = new Map();
  for (const r of rows) {
    const name = r.strip?.workflow_name;
    if (!name) continue;
    counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => (a.name < b.name ? -1 : 1));
}

/**
 * One predicate for the whole filter bar: holder ∧ workflow ∧ text. Tags are
 * the shared `passesTagFilter`, composed by the view. An unknown holder word
 * keeps the row — hiding what we cannot classify is how goals get lost.
 */
export function matchesFilters(row, f = {}) {
  if (f.holder && HOLDERS.includes(f.holder) && row.holder !== f.holder) return false;
  if (f.workflow && row.strip?.workflow_name !== f.workflow) return false;
  if (f.q) {
    const hay = `${row.title ?? ""} ${row.statement ?? ""}`.toLowerCase();
    if (!hay.includes(f.q.toLowerCase())) return false;
  }
  return true;
}

/** Newest movement first; ties break by id (a ULID, so by birth), newest first. */
export function sortByActivity(rows) {
  return [...rows].sort((a, b) => {
    const at = b.last_activity_at - a.last_activity_at;
    if (at !== 0) return at;
    return a.id < b.id ? 1 : a.id > b.id ? -1 : 0;
  });
}

/**
 * The holder option that means "no holder filter" — an id of its own, so a
 * link that says `?holder=all` reads as no filter, as it always has. It is
 * not a holder word.
 */
export const HOLDER_FILTER_ALL = "all";

/** `?holder=&workflow=&q=&archived=1` — the filters live in the URL, not in storage. */
export function parseFilters(params) {
  const read = (k) => {
    const v = params?.get?.(k);
    return v ? v : undefined;
  };
  const holder = read("holder");
  return {
    holder: holder === HOLDER_FILTER_ALL ? undefined : holder,
    workflow: read("workflow"),
    q: read("q"),
    archived: read("archived") === "1" ? true : undefined,
  };
}

export function serializeFilters(f = {}) {
  const out = {};
  if (f.holder) out.holder = f.holder;
  if (f.workflow) out.workflow = f.workflow;
  if (f.q) out.q = f.q;
  if (f.archived) out.archived = "1";
  return out;
}

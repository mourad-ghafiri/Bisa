/**
 * A proposed workflow as the ask card reads it. The node's
 * `NeedsAction.proposal` carries the design — description, every step with a
 * one-line summary, the flows, the inputs — and this module turns it into the
 * rows and sentences the card shows, so a person judges the workflow itself
 * and not a name and a count.
 */

import { t as tr } from "../../i18n/l10n.mjs";

/** The card's headline: the name, how many steps, which revision. */
export function proposalHeadline(p) {
  const n = p?.steps?.length ?? 0;
  return tr("studio-proposal-step-steps-rev", { p: p?.name ?? tr("studio-proposal-untitled-workflow"), n, revision: p?.revision ?? 1 });
}

/** The steps as the reader walks them: `Plan → Build → Review → Ship`. */
export function flowSentence(p) {
  return (p?.steps ?? []).map((s) => s.name || s.id).join(" → ");
}

/**
 * One row per step: its kind, name and summary, who does it, and what departs
 * from the defaults — where it flows on failure, whether it joins on the first
 * arrival, how many visits a loop through it may make, which branches leave it.
 */
export function stepLines(p) {
  const edges = p?.edges ?? [];
  return (p?.steps ?? []).map((s, i) => {
    const notes = [];
    const branches = edges.filter((e) => e.from === s.id && e.kind === "then" && e.branch).map((e) => `${e.branch} → ${e.to}`);
    if (branches.length > 0) notes.push(tr("studio-proposal-branches", { branches: branches.join(", ") }));
    if (s.on_fail) notes.push(s.on_fail === "skip" ? tr("studio-proposal-on-failure-skip") : tr("studio-proposal-on-failure", { branch: s.on_fail }));
    if (s.join_any) notes.push(tr("studio-proposal-continues-when-first-flow-arrives"));
    if (typeof s.max_visits === "number" && s.max_visits !== 3) notes.push(tr("studio-proposal-up-visits", { max_visits: s.max_visits }));
    return {
      index: i + 1,
      id: s.id,
      name: s.name || s.id,
      kind: s.kind,
      summary: s.summary ?? null,
      assignee: s.assignee ?? null,
      notes,
    };
  });
}

/** The edges that flow *back* to an earlier step, by the steps' order — the loops a reader should know about. */
export function loopSentences(p) {
  const order = new Map((p?.steps ?? []).map((s, i) => [s.id, i]));
  return (p?.edges ?? [])
    .filter((e) => order.has(e.from) && order.has(e.to) && order.get(e.to) <= order.get(e.from))
    .map((e) => tr("studio-proposal-loops-back", { from: e.from, to: e.to, when: e.branch ? "branch" : e.kind === "on_fail" ? "fail" : "none", branch: e.branch ?? "" }));
}

/**
 * What the design starts on: its start steps' one-line summaries, in the
 * steps' order — *begins by hand*, *begins on the schedule …*, *begins when
 * called* — the words a person adopts a listening design by. Empty for a
 * design with no start step.
 */
export function startsOn(p) {
  return (p?.steps ?? []).filter((s) => s.kind === "start" && s.summary).map((s) => s.summary);
}

/** The event word of a start by hand — the core's `StartOn::Manual`. */
const MANUAL = "manual";

/**
 * Whether adopting the design makes its goal **listen** rather than run: one
 * of its starts begins on an event. A design whose starts are by hand, or
 * that names none, runs.
 */
export function listens(p) {
  return (p?.steps ?? []).some((s) => s.kind === "start" && typeof s.event === "string" && s.event !== MANUAL);
}

/**
 * The inputs an adoption asks, in the design's order: every input for a
 * design that runs; for one that listens, only what listening needs — the
 * inputs no event supplies and no default fills (`listening_needs`), since
 * the rest arrive with each event.
 */
export function adoptionInputs(p) {
  const inputs = p?.inputs ?? [];
  if (!listens(p)) return [...inputs];
  const needs = new Set(p?.listening_needs ?? []);
  return inputs.filter((i) => needs.has(i.name));
}

/**
 * How many inputs the adoption asks, and which must be filled before it can
 * go: the required ones without a default — every one a listening design
 * asks is such a one.
 */
export function inputsNeeded(p) {
  const inputs = adoptionInputs(p);
  const required = listens(p) ? inputs : inputs.filter((i) => i.required && (i.default === undefined || i.default === null));
  return { total: inputs.length, required: required.map((i) => i.label || i.name) };
}

/** What a change request must carry: some words. */
export function changeRequest(text) {
  const t = String(text ?? "").trim();
  return t.length > 0 ? t : null;
}

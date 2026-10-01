/**
 * The marks on the right panel's rail: what a tab's icon says about its
 * root while its column is closed, so a person knows there is something to
 * commit or discard, or that the branch's story is moving, without opening it.
 *
 * A mark is a dot with a sentence — the dot's whole meaning is in its label,
 * as the project rail's is. Three tones: `accent` (something is in hand),
 * `danger` (something is wrong — conflicts), `working` (something is
 * happening right now — an agent, a push, a merge).
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The Git tab: the checkout has something to commit or discard.
 *
 * @param {{ git?: boolean, exists?: boolean, staged: number, unstaged: number, untracked: number, conflicted: number } | null | undefined} status
 * @returns {{ tone: "accent" | "danger" | "working", title: string } | null}
 */
export function gitBadge(status) {
  if (!status || status.git === false || status.exists === false) return null;
  if (status.conflicted > 0) {
    return { tone: "danger", title: t("workbench-rail-badges-conflict-conflicts-resolve", { conflicted: status.conflicted }) };
  }
  const n = (status.staged ?? 0) + (status.unstaged ?? 0) + (status.untracked ?? 0);
  if (n <= 0) return null;
  return { tone: "accent", title: t("workbench-rail-badges-change-changes-commit-discard", { n }) };
}

/** What a git act in flight is called while it runs. */
const BUSY_WORDS = Object.freeze({ push: "pushing…", pr: t("workbench-rail-badges-opening-pull-request"), merge: "merging…" });

/**
 * The Workstreams tab: a step of the branch's lifecycle is in progress.
 *
 * Only a branch beside the primary has a lifecycle (`kind === "worktree"`);
 * the primary and a copy never wear the mark. In order of urgency: an agent
 * reviewing or fixing (its run starting or live), a push, pull request or
 * merge in flight, then a pull request open — its checks, review and merge
 * in hand. A branch merely ahead of its base is not in progress: nothing is
 * happening to it.
 *
 * @param {{
 *   kind: string | null | undefined,
 *   pr: { number: number } | null | undefined,
 *   busy: string | null | undefined,
 *   run: { run: { kind: string, agent: string }, starting: boolean, live: boolean, done: boolean } | null | undefined,
 * }} facts
 * @returns {{ tone: "accent" | "danger" | "working", title: string } | null}
 */
export function workstreamsBadge(facts) {
  if (facts.kind !== "worktree") return null;
  const run = facts.run;
  if (run && !run.done && (run.starting || run.live)) {
    const verb = run.run.kind === "fix" ? "fixing" : run.run.kind === "review" ? "reviewing" : t("workbench-rail-badges-working-branch");
    return { tone: "working", title: t("workbench-rail-badges-words", { agent: run.run.agent, verb }) };
  }
  if (facts.busy && facts.busy in BUSY_WORDS) return { tone: "working", title: BUSY_WORDS[facts.busy] };
  if (facts.pr) return { tone: "accent", title: t("workbench-rail-badges-pull-request-open-checks-review-merge", { number: facts.pr.number }) };
  return null;
}

/**
 * The tab's tooltip with its mark's sentence: *Show Git · 3 changes to
 * commit or discard*.
 *
 * @param {string} name the tab's name, with its chord when it has one
 * @param {boolean} showing
 * @param {{ title: string } | null | undefined} badge
 */
export function badgeTooltip(name, showing, badge) {
  const base = `${showing ? "Hide" : t("workbench-rail-badges-show")} ${name}`;
  return badge ? `${base} · ${badge.title}` : base;
}

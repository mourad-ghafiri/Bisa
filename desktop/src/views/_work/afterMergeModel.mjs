/**
 * What follows a merged pull request, decided from three policies
 * and one fact:
 *
 * - `workstreams.cleanup` — `keep` · `ask` · `remove_when_merged`: the
 *   workstream's checkout and local branch;
 * - `workstreams.after_merge` — `ask` · `return_and_pull` · `stay`: going back
 *   to the default branch and pulling the merge in;
 * - `git.pull` — how that pull moves the primary;
 * - and whether anything is **running on the primary** right now, because a
 *   pull under a harness's feet is the one thing this flow must never do.
 *
 * Deleting the merged workstream's checkout terminates what still stands in
 * it — its harnesses, shells and agent sessions (`closeWorkstreamModel`) —
 * and the delete step says so before the person confirms; the summary says
 * what went.
 *
 * The plan is a list of steps with a checkbox each; `mode` says whether to
 * show the dialog, run silently, or do nothing. Running is the component's
 * job; deciding is here, where a test can hold it.
 */

import { terminationWords } from "./closeWorkstreamModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export const CLEANUP = Object.freeze(["keep", "ask", "remove_when_merged"]);
export const AFTER_MERGE = Object.freeze(["ask", "return_and_pull", "stay"]);

/** The order steps run in: the pull happens on the primary *before* leaving. */
export const STEP_ORDER = Object.freeze(["pull", "delete", "return"]);

/**
 * @param {object} input
 * @param {string} input.cleanup the resolved `workstreams.cleanup`
 * @param {string} input.afterMerge the resolved `workstreams.after_merge`
 * @param {string} input.pullMode the resolved `git.pull`
 * @param {number | null} input.primaryBusy sessions running on the primary (server count + local harness shells); `null` when the primary could not be read — unknown is not idle
 * @param {string | null} input.defaultBranch the project's default branch
 * @param {string | null} input.branch the merged workstream's branch
 * @param {boolean} [input.dirty] the merged checkout has uncommitted changes
 * @param {import("./closeWorkstreamModel.mjs").TerminationCounts} [input.terminated] what still stands in the merged checkout
 * @returns {{mode: "none" | "silent" | "dialog", steps: Array<{id: string, label: string, detail: string, checked: boolean, available: boolean, reason: string | null}>}}
 */
export function afterMergePlan({ cleanup, afterMerge, pullMode, primaryBusy, defaultBranch, branch, dirty = false, terminated = NOTHING }) {
  const main = defaultBranch ?? t("work-after-merge-dialog-default-branch");
  const unknown = primaryBusy === null || primaryBusy === undefined;
  const busy = unknown || primaryBusy > 0;
  const wantsCleanup = cleanup !== "keep";
  const wantsReturn = afterMerge !== "stay";
  const busyReason = unknown
    ? t("work-after-merge-whether-anything-runs-could-not-read", { main })
    : busy
      ? t("work-after-merge-session-sessions-runs-run-stop-them", { primaryBusy, main })
      : null;
  const ends = terminationWords(terminated);

  const steps = [
    {
      id: "pull",
      label: t("work-after-merge-pull", { main, pullMode: PULL_WORD[pullMode] ?? pullMode }),
      detail: t("work-after-merge-bring-merge-into-machine-conflict-stops", { main }),
      checked: wantsReturn && !busy,
      available: !busy,
      reason: busyReason,
    },
    {
      id: "delete",
      label: t("work-after-merge-delete-workstream-s-checkout-branch-branch", { branch, flag: (branch) ? "yes" : "no" }),
      detail: [
        dirty
          ? t("work-after-merge-checkout-has-uncommitted-changes-they-saved")
          : t("work-after-merge-merge-code-host-local-branch-checkout"),
        ...(ends ? [t("work-after-merge-what-still-stands-ends", { ends })] : []),
      ].join(" "),
      checked: wantsCleanup,
      available: true,
      reason: null,
    },
    {
      id: "return",
      label: t("work-after-merge-return", { main }),
      detail: t("work-after-merge-open-project-s-primary-checkout-workbench"),
      checked: wantsReturn && !busy,
      available: !busy,
      reason: busyReason,
    },
  ];

  let mode = "dialog";
  if (!wantsCleanup && !wantsReturn) mode = "none";
  else if (cleanup !== "ask" && afterMerge !== "ask" && !(wantsReturn && busy)) mode = "silent";
  return { mode, steps };
}

const PULL_WORD = Object.freeze({ ff_only: "fast-forward", rebase: "rebase", merge: "merge" });

/** The ids of the steps to run, checked and available, in running order. */
export function stepsToRun(steps) {
  const chosen = new Set(steps.filter((s) => s.checked && s.available).map((s) => s.id));
  return STEP_ORDER.filter((id) => chosen.has(id));
}

/** Flip one step's checkbox; a step that is not available stays off. */
export function toggleStep(steps, id) {
  return steps.map((s) => (s.id === id && s.available ? { ...s, checked: !s.checked } : s));
}

/**
 * The words for a finished flow, from each step's outcome — and, when the
 * workstream went, what went with it.
 * @param {Record<string, "done" | "skipped" | "failed" | "conflict">} outcomes
 * @param {string | null} defaultBranch
 * @param {import("./closeWorkstreamModel.mjs").TerminationCounts} [terminated] what the delete step terminated
 */
export function summaryOf(outcomes, defaultBranch, terminated = NOTHING) {
  const main = defaultBranch ?? t("work-after-merge-dialog-default-branch");
  const parts = [];
  if (outcomes.pull === "done") parts.push(t("work-after-merge-pulled", { main }));
  if (outcomes.pull === "conflict") parts.push(t("work-after-merge-pull-stopped-conflicts", { main }));
  if (outcomes.pull === "failed") parts.push(t("work-after-merge-pull-failed"));
  if (outcomes.delete === "done") {
    const ends = terminationWords(terminated);
    parts.push(ends ? t("work-after-merge-workstream-removed", { ends }) : t("work-after-merge-workstream-removed-2"));
  }
  if (outcomes.delete === "failed") parts.push(t("work-after-merge-workstream-kept-removing-failed"));
  if (outcomes.return === "done") parts.push(t("work-after-merge-now", { main }));
  return parts.length === 0 ? t("work-after-merge-nothing-changed") : `${parts.join(", ")}.`.replace(/^./, (c) => c.toUpperCase());
}

/** No harness, no shell, no agent session: the counts of an empty checkout. */
const NOTHING = Object.freeze({ harnesses: 0, shells: 0, agents: 0 });

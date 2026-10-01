/**
 * The merge, as one control (ide/08): a **Merge** button with the strategy as
 * its caption — a menu only when the code host offers more than one, starting on
 * the project's `git.merge_strategy` — and a confirmation that says, in words,
 * what is about to happen and whether the branch on the code host goes with it.
 *
 * A merge leaves the machine and cannot be recalled, so it passes the
 * project's Publish gate exactly like a push; when the step is blocked the
 * button says why instead of refusing silently. A review is optional and
 * never gates this control; checks still running, comments still open and a
 * standing request for changes are **cautions**, not blocks: the button stays
 * live, the confirmation names them, and the act reads *Merge anyway*. The
 * control is drawn under the *Merge* step and nowhere else — the lifecycle's
 * one act sits under its own step.
 */

import { useState } from "react";
import type { CodeHostCapabilities, MergeStrategy, PullRequest } from "../../types";
import { Button, Checkbox, ConfirmDialog, ICON, Menu, Tooltip } from "../../ui";
import { defaultStrategy, mergeStrategies } from "./prFormModel.mjs";
import { ReasonLine } from "./ReasonLine";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

const STRATEGY_WORD: Record<string, string> = { merge: t("work-merge-control-merge-commit"), squash: "squash", rebase: "rebase" };

export function MergeControl({
  pr,
  caps,
  preferredStrategy,
  deleteBranchDefault,
  blocked,
  cautions,
  busy,
  onMerge,
}: {
  pr: PullRequest;
  caps: CodeHostCapabilities | null;
  /** The resolved `git.merge_strategy` — where the caption starts. */
  preferredStrategy: string;
  /** The resolved `git.delete_branch_after_merge` — where the toggle starts. */
  deleteBranchDefault: boolean;
  /** Why the merge is not legal yet, or null when it is. */
  blocked: string | null;
  /** What reviewers said that is worth a second look — the merge stays legal. */
  cautions: readonly string[];
  busy: boolean;
  onMerge: (strategy: MergeStrategy, deleteBranch: boolean) => void;
}) {
  const strategies = mergeStrategies(caps);
  const [strategy, setStrategy] = useState<MergeStrategy | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [deleteBranch, setDeleteBranch] = useState<boolean | null>(null);
  const chosen = (strategy ?? defaultStrategy(caps, preferredStrategy)) as MergeStrategy | null;
  if (!chosen) return <ReasonLine>{t("work-merge-control-code-host-does-not-merge-from")}</ReasonLine>;
  const willDelete = caps?.delete_branch === true && (deleteBranch ?? deleteBranchDefault);
  const cautioned = cautions.length > 0;

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Tooltip label={blocked ? "" : t("work-merge-control-through-project-s-publish-gate-like")}>
        <span className="inline-flex">
          <Button size="sm" variant="primary" disabled={busy || blocked !== null} onClick={() => setConfirming(true)}>
            {busy ? t("work-merge-control-merging") : t("work-merge-control-merge")}
          </Button>
        </span>
      </Tooltip>
      {strategies.length > 1 ? (
        <Menu
          label={t("work-merge-control-merge-strategy")}
          items={strategies.map((s) => ({ label: STRATEGY_WORD[s] ?? s, icon: s === chosen ? ICON.check : undefined, onSelect: () => setStrategy(s) }))}
          trigger={
            <span className="anim inline-flex h-6 items-center gap-1 rounded-control px-1.5 text-2xs text-text-dim hover:bg-surface-2 hover:text-text" title={t("work-merge-control-how-branch-lands-base")}>
              {STRATEGY_WORD[chosen] ?? chosen}
              <ICON.collapsed size={10} aria-hidden />
            </span>
          }
        />
      ) : (
        <span className="text-2xs text-text-dim">{STRATEGY_WORD[chosen] ?? chosen}</span>
      )}
      {!blocked && cautioned && (
        <span className="flex min-w-0 items-center gap-1 truncate text-2xs text-warn" title={cautions.join(" · ")}>
          <ICON.warn size={11} aria-hidden />
          {cautions.join(" · ")}
        </span>
      )}

      <ConfirmDialog
        open={confirming}
        onClose={() => setConfirming(false)}
        onConfirm={() => {
          setConfirming(false);
          onMerge(chosen, willDelete);
        }}
        title={t("work-merge-control-merge-into", { number: pr.number, base: pr.base })}
        confirmLabel={cautioned ? t("work-merge-control-merge-anyway") : t("work-merge-control-merge")}
        danger={cautioned}
        body={
          <>
            {cautioned && (
              <div className="mb-2 rounded-control border border-warn/40 bg-warn/5 px-2 py-1.5 text-2xs">
                <p className="font-medium text-warn">{t("work-merge-control-worth-second-look")}</p>
                <ul className="mt-0.5 list-disc pl-4">
                  {cautions.map((c) => (
                    <li key={c}>{c}</li>
                  ))}
                </ul>
                <p className="mt-1 text-text-dim">{t("work-merge-control-wait-checks-resolve-rest-under-review")}</p>
              </div>
            )}
            <p>{rich("work-merge-control-lands-on-as", { head: <span className="font-mono">{pr.head}</span>, base: <span className="font-mono">{pr.base}</span> }, { strategy: STRATEGY_WORD[chosen] ?? chosen, delete: willDelete ? "yes" : "no" })}</p>
            {caps?.delete_branch && (
              <div className="mt-2">
                <Checkbox label={t("work-merge-control-delete-code-host-after-merge", { head: pr.head })} checked={deleteBranch ?? deleteBranchDefault} onChange={setDeleteBranch} hint={t("work-merge-control-local-branch-checkout-asked-about-afterwards")} />
              </div>
            )}
          </>
        }
      />
    </div>
  );
}

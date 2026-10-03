/**
 * Closing a workstream (ide/07 §Operations), as the two dialogs every surface
 * shares: *Close, keep the checkout* — the record closes and stops taking
 * work, the branch and the folder stay — and, one step further, *Delete the
 * checkout* — the working tree goes from disk, named in full, with a word
 * about what it still holds. Either way, what stands in the workstream ends
 * with it — its harnesses terminated, its shells closed, its agent sessions
 * aborted — and each dialog says how many before the person confirms
 * (`terminationConsent`): this is the one consent, and the tabs close
 * without the terminal guard asking again. The Workstreams occupant and the
 * project's list both open these, so the words and the outcomes are one.
 */

import { Button, ConfirmDialog, Dialog, LinkedText } from "../../ui";
import { terminationConsent } from "./closeWorkstreamModel.mjs";
import type { TerminationCounts } from "./closeWorkstreamModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

/** Which dialog is up: none, the record's, or the destructive one. */
export type Closing = null | "record" | "tree";

export function CloseWorkstreamDialogs({
  mode,
  onModeChange,
  path,
  clean,
  branch,
  busy,
  terminated,
  onClose,
  problem = null,
}: {
  mode: Closing;
  onModeChange: (mode: Closing) => void;
  /** The checkout's path, when the surface knows it. */
  path: string | null;
  /** Whether the tree has nothing uncommitted; `null` when unknown. */
  clean: boolean | null;
  /** The branch that survives a deleted checkout, when there is one. */
  branch: string | null;
  busy: boolean;
  /** What stands in the workstream now — said before the person confirms. */
  terminated: TerminationCounts;
  /** Close the record; `tree` also removes the checkout from disk. */
  onClose: (tree: boolean) => void;
  /**
   * The clean script refused the last delete (ide/07 §Workstream scripts):
   * its words, shown in the delete dialog so the person reads them where they
   * decide, rather than in a toast that is gone before the retry.
   */
  problem?: { title: string; message: string; output: string } | null;
}) {
  const ends = terminationConsent(terminated);
  return (
    <>
      <Dialog
        open={mode === "record"}
        onClose={() => onModeChange(null)}
        title={t("work-close-workstream-dialogs-close-workstream")}
        width="max-w-md"
        footer={
          <>
            <Button variant="ghost" onClick={() => onModeChange(null)}>{t("work-agent-editor-cancel")}</Button>
            <Button
              variant="primary"
              disabled={busy}
              onClick={() => {
                onModeChange(null);
                onClose(false);
              }}
            >{t("work-close-workstream-dialogs-close-keep-checkout")}</Button>
          </>
        }
      >
        <div className="flex flex-col gap-2 text-xs text-text-dim">
          <p>{t("work-close-workstream-dialogs-workstream-marked-closed-stops-taking-work")}</p>
          {ends && <p className="text-text">{ends}</p>}
          <button type="button" onClick={() => onModeChange("tree")} className="anim rounded-control border border-border px-2 py-1.5 text-left text-2xs text-danger hover:bg-danger-soft">{t("work-close-workstream-dialogs-remove-checkout-from-disk-well")}</button>
        </div>
      </Dialog>

      <ConfirmDialog
        open={mode === "tree"}
        onClose={() => onModeChange(null)}
        danger
        title={t("work-close-workstream-dialogs-delete-checkout")}
        confirmLabel={t("work-close-workstream-dialogs-delete-checkout-2")}
        body={
          <>
            <p>
              {path ? rich("work-close-workstream-dialogs-removes-working-tree-at-path", { path: <code className="break-all">{path}</code> }) : t("work-close-workstream-dialogs-removes-working-tree")}
            </p>
            <p className="mt-1.5">
              {clean === true ? t("work-close-workstream-dialogs-there-nothing-uncommitted-here") : clean === false ? t("work-close-workstream-dialogs-has-uncommitted-changes-they-only-copy") : t("work-close-workstream-dialogs-anything-uncommitted-only-copy-goes")}{" "}
              {branch ? t("work-close-workstream-dialogs-branch-survives-repository-checkout-does-not", { branch }) : ""}
            </p>
            {ends && <p className="mt-1.5">{ends}</p>}
            {problem && (
              <div className="mt-2 rounded-control border border-danger/40 bg-danger-soft p-2 text-2xs">
                <p className="font-medium text-danger">{problem.title}</p>
                <p className="text-text-dim">{t("work-close-workstream-dialogs-checkout-stays-until-script-succeeds-fix", { message: problem.message })}</p>
                {problem.output && <LinkedText as="pre" className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap font-mono text-2xs text-text" text={problem.output} />}
              </div>
            )}
          </>
        }
        onConfirm={() => onClose(true)}
      />
    </>
  );
}

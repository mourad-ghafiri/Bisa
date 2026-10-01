/**
 * A forced push for a rewritten branch — `--force-with-lease`, never the
 * project's default branch, through the Publish gate, consented. One dialog,
 * so the promise it makes is made once: the remote is overwritten only if it
 * still points where it did when you last fetched, and what is here is saved
 * first (`SafetyNote`). The words are `branchActionsModel.mjs`'s; the push
 * runs through `gitOps` against the checkout's session.
 */
import { ConfirmDialog } from "../../ui";
import { rootKey } from "../_workbench/workbenchModel.mjs";
import { consentWords, doneWords } from "./branchActionsModel.mjs";
import * as ops from "./gitOps";
import { SafetyNote } from "./SafetyNote";
import { t } from "../../i18n/l10n.mjs";

export function PushWithLeaseDialog({ open, onClose, wid, branch }: { open: boolean; onClose: () => void; wid: string; branch: string | null }) {
  const copy = consentWords("push_lease", { name: branch ?? t("work-push-with-lease-dialog-branch") });
  const scope = rootKey("workstream", wid);
  return (
    <ConfirmDialog
      open={open}
      onClose={onClose}
      onConfirm={() => {
        onClose();
        void ops.pushWithLease(scope, wid, branch ?? t("work-cherry-pick-dialog-branch"), doneWords("push_lease", { name: branch ?? t("work-cherry-pick-dialog-branch") }));
      }}
      title={copy.title}
      body={
        <div className="flex flex-col gap-2">
          <p>{copy.body}</p>
          <SafetyNote kind={copy.kind} />
        </div>
      }
      confirmLabel={copy.confirm}
      danger={copy.danger}
    />
  );
}

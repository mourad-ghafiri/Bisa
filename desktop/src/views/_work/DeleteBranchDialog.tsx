/**
 * The one dialog a local branch is deleted from (ide/04 §The Branches
 * view): the question, the promise — its tip is pinned in Safety — and,
 * when the branch follows a remote one, *Also delete it on <remote>*, which
 * is the outward act through the project's publishing policy.
 */
import { useEffect, useState } from "react";
import type { BranchInfo } from "../../types";
import { Button, Checkbox, Dialog, SOLID_DANGER } from "../../ui";
import { consentWords } from "./branchActionsModel.mjs";
import { SafetyNote } from "./SafetyNote";
import { t } from "../../i18n/l10n.mjs";

/** `origin/feature` → `{remote: "origin", name: "feature"}`; null for no upstream. */
function upstreamParts(upstream: string | null | undefined): { remote: string; name: string } | null {
  if (!upstream) return null;
  const at = upstream.indexOf("/");
  if (at <= 0) return null;
  return { remote: upstream.slice(0, at), name: upstream.slice(at + 1) };
}

export function DeleteBranchDialog({
  open,
  onClose,
  branch,
  busy = false,
  onConfirm,
}: {
  open: boolean;
  onClose: () => void;
  branch: BranchInfo | null;
  busy?: boolean;
  onConfirm: (alsoRemote: { remote: string; name: string } | null) => void;
}) {
  const remote = upstreamParts(branch?.upstream);
  const [alsoRemote, setAlsoRemote] = useState(false);
  useEffect(() => {
    if (open) setAlsoRemote(false);
  }, [open, branch?.name]);
  const copy = consentWords("delete_branch", { name: branch?.name ?? "" });
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={copy.title}
      description={copy.body}
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          {/* A destructive confirm fills solid danger, as the kit's `ConfirmDialog` does (The Irreversible Asks Rule). */}
          <Button variant="primary" className={SOLID_DANGER} disabled={busy} onClick={() => onConfirm(alsoRemote && remote ? remote : null)}>
            {busy ? t("work-delete-branch-dialog-deleting") : copy.confirm}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        {remote && (
          <Checkbox
            label={t("work-delete-branch-dialog-also-delete", { remote: remote.remote })}
            hint={t("work-delete-branch-dialog-goes-too-through-project-s-publishing", { remote: remote.remote, remote2: remote.name })}
            checked={alsoRemote}
            onChange={setAlsoRemote}
          />
        )}
        <SafetyNote kind="commit" />
      </div>
    </Dialog>
  );
}

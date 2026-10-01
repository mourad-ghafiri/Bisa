/**
 * The one dialog a branch's upstream is set in (ide/04 §The Branches view):
 * a remote branch to follow — the one it follows now picked — or none.
 * Configuration, not history: nothing moves and nothing is pinned.
 */
import { useEffect, useState } from "react";
import type { BranchInfo } from "../../types";
import { Button, Dialog, Field, Select } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

const NONE = "";

export function UpstreamDialog({
  open,
  onClose,
  branch,
  remoteBranches,
  busy = false,
  onConfirm,
}: {
  open: boolean;
  onClose: () => void;
  branch: BranchInfo | null;
  /** Every remote branch, as `remote/name`. */
  remoteBranches: string[];
  busy?: boolean;
  onConfirm: (upstream: string | null) => void;
}) {
  const [upstream, setUpstream] = useState(branch?.upstream ?? NONE);
  useEffect(() => {
    if (open) setUpstream(branch?.upstream ?? NONE);
  }, [open, branch?.name, branch?.upstream]);
  const choices = remoteBranches.includes(upstream) || upstream === NONE ? remoteBranches : [upstream, ...remoteBranches];
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("work-upstream-dialog-upstream", { branch: branch?.name ?? "" })}
      description={t("work-upstream-dialog-remote-branch-one-follows-what-push")}
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={busy || upstream === (branch?.upstream ?? NONE)} onClick={() => onConfirm(upstream === NONE ? null : upstream)}>
            {busy ? t("work-agent-editor-saving") : t("work-agent-editor-save")}
          </Button>
        </>
      }
    >
      <Field label={t("work-upstream-dialog-follows")}>
        <Select value={upstream} onChange={(e) => setUpstream(e.target.value)} className="font-mono">
          <option value={NONE}>{t("work-upstream-dialog-no-upstream")}</option>
          {choices.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </Select>
      </Field>
    </Dialog>
  );
}

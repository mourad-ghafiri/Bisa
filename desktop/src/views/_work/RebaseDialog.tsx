/**
 * The one dialog a rebase is asked for in (ide/04 §The Branches view): the
 * current branch onto a target, with *Stash local changes first and bring
 * them back* (git's autostash — on when the tree is dirty) and *Only the
 * commits since…* (git's `--onto`: the commits since one branch replayed
 * onto another). The consenting **Rebase** carries the `SafetyNote`.
 */
import { useEffect, useState } from "react";
import { Button, Checkbox, Dialog, Field, TextInput } from "../../ui";
import { LookAhead } from "./MergeDialog";
import { rebaseConsent } from "./mergeModel.mjs";
import { SafetyNote } from "./SafetyNote";
import { t } from "../../i18n/l10n.mjs";

export function RebaseDialog({
  open,
  onClose,
  wid,
  current,
  target,
  dirty,
  busy = false,
  onConfirm,
}: {
  open: boolean;
  onClose: () => void;
  wid: string;
  current: string;
  /** What the branch is rebased onto — local, or `origin/x`. */
  target: string;
  /** The tree has uncommitted changes: autostash starts on. */
  dirty: boolean;
  busy?: boolean;
  onConfirm: (body: { upstream: string; onto: string | null; autostash: boolean }) => void;
}) {
  const [autostash, setAutostash] = useState(dirty);
  const [sinceOn, setSinceOn] = useState(false);
  const [since, setSince] = useState("");
  useEffect(() => {
    if (open) {
      setAutostash(dirty);
      setSinceOn(false);
      setSince("");
    }
  }, [open, dirty, target]);
  const onto = sinceOn && since.trim() ? target : null;
  const upstream = onto ? since.trim() : target;
  const copy = rebaseConsent(current, upstream, { autostash, onto });
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
          <Button variant="primary" disabled={busy || (sinceOn && !since.trim())} onClick={() => onConfirm({ upstream, onto, autostash })}>
            {busy ? t("work-rebase-dialog-rebasing") : copy.confirm}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <LookAhead wid={wid} source={target} open={open} rebase />
        <Checkbox label={t("work-rebase-dialog-stash-local-changes-first-bring-them")} hint={dirty ? t("work-rebase-dialog-tree-has-uncommitted-changes-without-rebase") : t("work-rebase-dialog-tree-clean-nothing-needs-stashing")} checked={autostash} onChange={setAutostash} />
        <Checkbox label={t("work-rebase-dialog-only-commits-since")} hint={t("work-rebase-dialog-replay-just-commits-has-since-another", { current, target })} checked={sinceOn} onChange={setSinceOn} />
        {sinceOn && (
          <Field label={t("work-rebase-dialog-since")} hint={t("work-rebase-dialog-branch-tag-commit-commits-after-ones")}>
            <TextInput autoFocus value={since} /* for the machine */ placeholder="main" className="font-mono" onChange={(e) => setSince(e.target.value)} />
          </Field>
        )}
        <SafetyNote kind="tree" />
      </div>
    </Dialog>
  );
}

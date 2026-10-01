/**
 * The one dialog a branch is named in from the Branches view (ide/04 §The
 * Branches view): the name, checked as it is typed (`refNameProblem`),
 * **start at** — HEAD, or any branch, tag or commit typed or picked — and
 * *Switch to it*. Creating is a ref, nothing moves; the switch half is
 * consented and says so.
 */
import { useEffect, useState } from "react";
import { Button, Checkbox, Dialog, Field, TextInput } from "../../ui";
import { refNameProblem } from "./commitActionsModel.mjs";
import { SafetyNote } from "./SafetyNote";
import { t } from "../../i18n/l10n.mjs";

export function NewBranchDialog({
  open,
  onClose,
  current,
  starts,
  busy = false,
  onConfirm,
}: {
  open: boolean;
  onClose: () => void;
  /** The current branch, the start point by default. */
  current: string | null;
  /** Every place a branch may start at — branches, remote branches, tags. */
  starts: string[];
  busy?: boolean;
  onConfirm: (body: { name: string; start: string | null; switch: boolean }) => void;
}) {
  const [name, setName] = useState("");
  const [start, setStart] = useState("");
  const [switchTo, setSwitchTo] = useState(true);
  useEffect(() => {
    if (open) {
      setName("");
      setStart("");
      setSwitchTo(true);
    }
  }, [open]);
  const problem = name.trim() === "" ? null : refNameProblem(name.trim());
  const ok = name.trim() !== "" && problem === null && !busy;
  const submit = () => {
    if (ok) onConfirm({ name: name.trim(), start: start.trim() || null, switch: switchTo });
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("work-new-branch-dialog-new-branch")}
      description={t("work-new-branch-dialog-unless-another-start-named-branch-name", { current: current ?? t("work-new-branch-dialog-head") })}
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={!ok} onClick={submit}>
            {busy ? t("work-new-branch-dialog-creating") : switchTo ? t("work-new-branch-dialog-create-switch") : t("work-agent-editor-create")}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <Field label={t("work-agent-editor-name")} hint={problem ?? undefined}>
          <TextInput autoFocus value={name} /* for the machine */ placeholder="feature/from-here" className="font-mono" aria-invalid={problem !== null || undefined} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label={t("work-new-branch-dialog-start")} hint={t("work-new-branch-dialog-branch-remote-branch-tag-commit-empty")}>
          <TextInput value={start} list="new-branch-starts" /* for the machine */ placeholder={current ?? "HEAD"} className="font-mono" onChange={(e) => setStart(e.target.value)} />
          <datalist id="new-branch-starts">
            {starts.map((s) => (
              <option key={s} value={s} />
            ))}
          </datalist>
        </Field>
        <Checkbox label={t("work-new-branch-dialog-switch")} hint={t("work-new-branch-dialog-tree-moves-new-branch-asked-here")} checked={switchTo} onChange={setSwitchTo} />
        {switchTo && <SafetyNote kind="tree" />}
      </form>
    </Dialog>
  );
}

/**
 * The one dialog a tag is named in (ide/04, ide/05) — from the Branches
 * view's *Tag HEAD…* and from a commit's *Tag here…* alike, so the two doors
 * refuse the same bad name with the same sentence (`refNameProblem`) and say
 * the same thing about a message: without one the tag is lightweight, with
 * one it is annotated, with you as its tagger. Nothing moves either way.
 */
import { useEffect, useState } from "react";
import { Button, Dialog, Field, TextInput } from "../../ui";
import { refNameProblem } from "./commitActionsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function TagDialog({
  open,
  onClose,
  target,
  description,
  busy = false,
  onSubmit,
}: {
  open: boolean;
  onClose: () => void;
  /** What is being tagged — a short sha, or *HEAD*. */
  target: string;
  /** The commit's subject, when known. */
  description?: string;
  busy?: boolean;
  onSubmit: (name: string, message: string) => void;
}) {
  const [name, setName] = useState("");
  const [message, setMessage] = useState("");
  // A fresh dialog starts empty, whichever commit asked last.
  useEffect(() => {
    if (open) {
      setName("");
      setMessage("");
    }
  }, [open, target]);
  const problem = name.trim() === "" ? null : refNameProblem(name.trim());
  const ok = name.trim() !== "" && problem === null && !busy;
  const submit = () => {
    if (ok) onSubmit(name.trim(), message.trim());
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("work-tag-dialog-tag", { target })}
      description={description}
      width="max-w-sm"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={!ok} onClick={submit}>
            {busy ? t("work-tag-dialog-tagging") : t("work-new-workstream-dialog-tag")}
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
        <Field label={t("work-agent-editor-name")} hint={problem ?? t("work-tag-dialog-nothing-moves-tag-name-commit")}>
          <TextInput autoFocus value={name} /* for the machine */ placeholder="v1.2.0" className="font-mono" aria-invalid={problem !== null || undefined} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label={t("work-merge-dialog-message")} hint={t("work-tag-dialog-optional-makes-tag-annotated-tagger")}>
          <TextInput value={message} placeholder={t("work-tag-dialog-release-1-2-0")} onChange={(e) => setMessage(e.target.value)} />
        </Field>
      </form>
    </Dialog>
  );
}

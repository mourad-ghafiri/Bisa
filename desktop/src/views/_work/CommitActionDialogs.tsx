/**
 * The questions a commit action asks (ide/05), drawn once for the whole
 * graph: the confirmation of a consented action — checkout, cherry-pick,
 * revert, switching to a branch, deleting a tag — the name a new branch
 * asks for, and the name and message a new tag asks for (`TagDialog`, the
 * same one the Branches view uses). A cherry-pick offers *Record where it
 * came from*; a merge commit picked or reverted asks which parent to keep.
 * What is asked is `useCommitActions`'s `pending`; the words are
 * `commitActionsModel.mjs`'s, and a name is refused inline by
 * `refNameProblem` before the round trip. The recovery promise is the
 * dialog's `SafetyNote`, said once.
 */
import { useEffect, useState } from "react";
import { Checkbox, ConfirmDialog, Field, PromptDialog, Select } from "../../ui";
import { consentWords, refNameProblem } from "./commitActionsModel.mjs";
import type { ConsentedKind } from "./commitActionsModel.mjs";
import { SafetyNote } from "./SafetyNote";
import { TagDialog } from "./TagDialog";
import type { CommitActions } from "./useCommitActions";
import { t } from "../../i18n/l10n.mjs";

export function CommitActionDialogs({ actions }: { actions: CommitActions }) {
  const { pending, busy, cancel, confirm, submitName } = actions;
  const consented = pending && pending.kind !== "branch" && pending.kind !== "tag" ? { ...pending, kind: pending.kind as ConsentedKind } : null;
  const parents = consented?.row.parents ?? 1;
  const copy = consented ? consentWords(consented.kind, { short: consented.row.short, name: consented.ref?.name ?? "", parents }) : null;
  const tagging = pending?.kind === "tag" ? pending : null;
  const asksMainline = consented !== null && (consented.kind === "cherry_pick" || consented.kind === "revert") && parents > 1;
  const [mainline, setMainline] = useState(1);
  const [recordOrigin, setRecordOrigin] = useState(false);
  useEffect(() => {
    if (consented) {
      setMainline(1);
      setRecordOrigin(false);
    }
    // Keyed on which dialog opened: the object is rebuilt each render, the
    // kind and the commit are what a new opening is.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [consented?.kind, consented?.row.id]);
  return (
    <>
      <ConfirmDialog
        open={consented !== null}
        onClose={cancel}
        onConfirm={() => confirm({ mainline: asksMainline ? mainline : null, recordOrigin })}
        title={copy?.title ?? ""}
        body={
          <div className="flex flex-col gap-2">
            <p>{copy?.body ?? ""}</p>
            {asksMainline && (
              <Field label={t("work-commit-action-dialogs-keep-side-parent")} hint={t("work-commit-action-dialogs-merge-commit-has-two-histories-change")}>
                <Select value={String(mainline)} onChange={(e) => setMainline(Number(e.target.value))}>
                  {Array.from({ length: parents }, (_, i) => (
                    <option key={i + 1} value={String(i + 1)}>
                      {i + 1}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
            {consented?.kind === "cherry_pick" && <Checkbox label={t("work-commit-action-dialogs-record-where-came-from")} hint={t("work-commit-action-dialogs-new-commit-s-message-ends-picked")} checked={recordOrigin} onChange={setRecordOrigin} />}
            <SafetyNote kind={consented?.kind === "delete_tag" ? "commit" : "tree"} />
          </div>
        }
        confirmLabel={copy?.confirm ?? t("work-commit-action-dialogs-continue")}
        danger={copy?.danger ?? false}
      />
      <PromptDialog
        open={pending?.kind === "branch"}
        onClose={cancel}
        title={t("work-commit-action-dialogs-new-branch", { short: pending?.row.short ?? "" })}
        description={pending?.row.subject}
        label={t("work-commit-action-dialogs-branch-name")}
        hint={t("work-commit-action-dialogs-nothing-moves-branch-name-commit")}
        placeholder="feature/from-here" // for the machine
        mono
        submitLabel={t("work-agent-editor-create")}
        busy={busy}
        validate={(v) => refNameProblem(v.trim())}
        onSubmit={(name) => submitName(name.trim())}
      />
      <TagDialog open={tagging !== null} onClose={cancel} target={tagging?.row.short ?? ""} description={tagging?.row.subject} busy={busy} onSubmit={(name, message) => submitName(name, message)} />
    </>
  );
}

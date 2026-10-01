/**
 * The *Open pull request* dialog, rendered from what the code host can do and from
 * nothing else (ide/08): title and body always; draft, reviewers and labels
 * each present only when `capabilities()` has the thing behind it — absent,
 * never greyed. The facts are `prFormModel.mjs`'s; this file is the form.
 *
 * The title starts as the branch tip's commit subject (`prTitleFrom`), so the
 * common case — one commit, one pull request — is a click. One button, and it
 * says when a push comes first: the node pushes an unpushed branch under the
 * same gate, and one decision covers both.
 */

import { useEffect, useState } from "react";
import type { CodeHostCapabilities, RepoRef } from "../../types";
import { Button, Checkbox, Dialog, Field, TextArea, TextInput } from "../../ui";
import { ReasonLine } from "./ReasonLine";
import { prNoun } from "./codeHostWords.mjs";
import { controlsFor, prActionLabel, prRequest } from "./prFormModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export interface PrFormValues {
  title: string;
  body?: string;
  draft?: boolean;
  reviewers?: string[];
  labels?: string[];
}

export function PrForm({
  open,
  onClose,
  caps,
  repo,
  codeHost,
  hostKnown,
  needsPush,
  initialTitle,
  busy,
  onSubmit,
}: {
  open: boolean;
  onClose: () => void;
  caps: CodeHostCapabilities | null;
  repo: RepoRef | null;
  /** The code host behind origin, by kind — for its name and its noun (a *merge request* on GitLab). */
  codeHost: string | null;
  /** The code host behind origin is one this build knows; otherwise the node refuses. */
  hostKnown: boolean;
  /** The branch is not on the remote yet: the button says the push happens first. */
  needsPush: boolean;
  initialTitle: string;
  busy: boolean;
  onSubmit: (values: PrFormValues) => void;
}) {
  const controls = new Set(controlsFor(caps));
  const [title, setTitle] = useState(initialTitle);
  const [body, setBody] = useState("");
  const [draft, setDraft] = useState(false);
  const [reviewers, setReviewers] = useState("");
  const [labels, setLabels] = useState("");
  // A fresh opening starts from the branch again, not from the last attempt.
  useEffect(() => {
    if (!open) return;
    setTitle(initialTitle);
    setBody("");
    setDraft(false);
    setReviewers("");
    setLabels("");
  }, [open, initialTitle]);

  const submit = () => {
    if (!title.trim() || busy) return;
    onSubmit(prRequest(caps, { title, body, draft, reviewers, labels }));
  };
  const where = repo ? `${repo.host}/${repo.owner}/${repo.name}` : t("work-pr-form-code-host-behind-origin");

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("work-pr-form-open", { codeHost: prNoun(codeHost) })}
      description={t("work-pr-form-through-project-s-publishing-policy-branch", { where, flag: (needsPush) ? "yes" : "no" })}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={!title.trim() || busy} onClick={submit}>
            {busy ? t("work-after-merge-dialog-working") : prActionLabel(needsPush ? "committed" : "pushed", prNoun(codeHost))}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label={t("work-pr-form-title")}>
          <TextInput
            autoFocus
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                submit();
              }
            }}
            placeholder={t("work-pr-form-fix-cart-total-rounding")}
          />
        </Field>
        <Field label={t("work-pr-form-body")} hint={t("work-pr-form-optional-markdown")}>
          <TextArea value={body} rows={4} onChange={(e) => setBody(e.target.value)} />
        </Field>
        {/* Each control below exists only when the code host has the capability
            behind it — absent, never greyed (ide/08). */}
        {controls.has("draft") && <Checkbox label={t("work-pr-form-draft")} checked={draft} onChange={setDraft} hint={t("work-pr-form-reviewers-see-cannot-merge-until-marked")} />}
        {controls.has("reviewers") && (
          <Field label={t("work-pr-form-reviewers")} hint={t("work-pr-form-handles-separated-commas-spaces")}>
            <TextInput value={reviewers} placeholder={t("work-pr-form-ada-grace")} className="font-mono" onChange={(e) => setReviewers(e.target.value)} />
          </Field>
        )}
        {controls.has("labels") && (
          <Field label={t("work-pr-form-labels")} hint={t("work-pr-form-separated-commas-spaces")}>
            <TextInput value={labels} /* content, never translated: an example of labels */ placeholder="ui, needs-review" className="font-mono" onChange={(e) => setLabels(e.target.value)} />
          </Field>
        )}
        {!hostKnown && <ReasonLine tone="warn">{t("work-pr-form-origin-not-code-host-build-knows")}</ReasonLine>}
        <ReasonLine>{t("work-pr-form-gated-project-opens-gate")}</ReasonLine>
      </div>
    </Dialog>
  );
}

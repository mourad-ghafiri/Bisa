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
 *
 * *Suggest* asks the General Agent for the title and the body from the
 * branch's commits and its changes, as the commit box asks for a message —
 * read-only, it cannot push or open anything. The draft lands in the fields
 * to be read and edited; a field typed in while it was asked keeps what was
 * typed (`applyPrSuggestion`), *Undo* puts back what was there, and closing
 * the dialog drops the ask.
 */

import { useEffect, useRef, useState } from "react";
import type { CodeHostCapabilities, RepoRef, SuggestedPullRequest } from "../../types";
import { Button, Checkbox, Dialog, Field, ICON, TextArea, TextInput, Tooltip, WorkingDot, failureText } from "../../ui";
import { ReasonLine } from "./ReasonLine";
import { prNoun } from "./codeHostWords.mjs";
import { applyPrSuggestion, controlsFor, draftedWords, prActionLabel, prRequest, prSuggestionOutcome } from "./prFormModel.mjs";
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
  suggest,
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
  /** Ask for a drafted title and body (`api.suggestPr`); absent, the form offers no *Suggest*. */
  suggest?: (signal: AbortSignal) => Promise<SuggestedPullRequest>;
}) {
  const controls = new Set(controlsFor(caps));
  const [title, setTitle] = useState(initialTitle);
  const [body, setBody] = useState("");
  const [draft, setDraft] = useState(false);
  const [reviewers, setReviewers] = useState("");
  const [labels, setLabels] = useState("");
  // The ask in the air, and what the fields held before a draft landed.
  const [asking, setAsking] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [undo, setUndo] = useState<{ title: string; body: string } | null>(null);
  const ask = useRef<AbortController | null>(null);
  // What the fields hold when a draft lands — read then, not when it was asked.
  const fields = useRef({ title, body });
  fields.current = { title, body };
  // A fresh opening starts from the branch again, not from the last attempt;
  // a closing drops whatever was still being asked.
  useEffect(() => {
    ask.current?.abort();
    ask.current = null;
    setAsking(false);
    setNote(null);
    setUndo(null);
    if (!open) return;
    setTitle(initialTitle);
    setBody("");
    setDraft(false);
    setReviewers("");
    setLabels("");
  }, [open, initialTitle]);
  useEffect(() => () => ask.current?.abort(), []);

  const askForDraft = async () => {
    if (!suggest || ask.current) return;
    const controller = new AbortController();
    ask.current = controller;
    const asked = { ...fields.current };
    setAsking(true);
    setNote(null);
    setUndo(null);
    try {
      const outcome = prSuggestionOutcome(await suggest(controller.signal));
      if (controller.signal.aborted) return;
      if (!outcome.draft) {
        setNote(outcome.note);
        return;
      }
      const now = { ...fields.current };
      const landed = applyPrSuggestion({ asked, now, draft: outcome.draft });
      setTitle(landed.title);
      setBody(landed.body);
      if (landed.title !== now.title || landed.body !== now.body) setUndo(now);
      setNote(draftedWords(landed.kept, prNoun(codeHost)));
    } catch (e) {
      // The route is always 200: reaching here is the node itself not answering.
      if (!controller.signal.aborted) setNote(failureText("git", "pr-suggest-failed", e));
    } finally {
      if (ask.current === controller) {
        ask.current = null;
        setAsking(false);
      }
    }
  };
  const putBack = () => {
    if (!undo) return;
    setTitle(undo.title);
    setBody(undo.body);
    setUndo(null);
    setNote(null);
  };

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
        <Field
          label={t("work-pr-form-title")}
          action={
            suggest && (
              <Tooltip label={t("work-pr-form-suggest-hint")}>
                <span className="inline-flex">
                  <Button size="sm" variant="ghost" className="h-6" disabled={asking || busy} onClick={() => void askForDraft()}>
                    <ICON.agent size={12} aria-hidden />
                    {asking ? t("work-agent-review-request-asking") : t("work-pr-form-suggest")}
                  </Button>
                </span>
              </Tooltip>
            )
          }
        >
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
          {/* Grows with a drafted body, to a point; ⌘⏎ opens, as it commits in the commit box. */}
          <TextArea
            value={body}
            rows={Math.min(12, Math.max(4, body.split("\n").length + 1))}
            onChange={(e) => setBody(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                e.preventDefault();
                submit();
              }
            }}
          />
        </Field>
        {suggest && (
          // Mounted while the form is, so the first sentence in it is announced:
          // the ask in the air, then the draft's word (and *Undo*) or why there is none.
          <p role="status" className="-mt-1.5 text-2xs text-text-dim empty:hidden">
            {asking ? (
              <span className="inline-flex items-center gap-1.5">
                <WorkingDot />
                {t("work-pr-form-asking")}
              </span>
            ) : (
              note
            )}
            {undo && !asking && (
              <button type="button" onClick={putBack} className="ml-1.5 font-medium text-text underline underline-offset-2 hover:no-underline">
                {t("work-pr-form-undo")}
              </button>
            )}
          </p>
        )}
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

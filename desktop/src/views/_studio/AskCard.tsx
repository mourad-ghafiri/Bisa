/**
 * An open ask at the foot of a conversation's timeline (ide/09, 11 —
 * Security). Four things are asked about: a tool call the Tool & Commands
 * Guard put to a person rather than deciding — a call no guard rule decided
 * in manual or auto, or any command in plan — and content from outside the
 * content screen held before the agent read it: a page or a review, with
 * its source, the reason (the classifier's sentence, or no verdict) and an
 * excerpt, so the person sees the risk in the agent's own place. **Allow
 * once** and **Allow for this conversation** / **Allow this site for this
 * conversation** (only when `grantable`) let it through; **Deny**, with an
 * optional note, refuses it and the agent goes on with a sentence. In a
 * conversation with no goal to escalate to, the agent's own **question** in
 * words — answered with the words typed or a choice it offered, or
 * declined — and a **sign-in** it needs — *Signed in* or *Not now*, the URL
 * shown as text and never navigated (ide/17) — wait on the same card.
 *
 * Driven by `ask_opened` / `ask_settled` plus one `GET` on mount
 * (`useConversationAsks`, `_workbench/reviewStore` for the changes side —
 * this card keeps its own small read since an ask is not a change). A URL is
 * shown as text, never navigated (ide/17).
 */

import { useState } from "react";
import { api } from "../../api";
import type { AskSubjectAsked, AskView, AnswerAskBody } from "../../types";
import { allowBody, answerBody, denyBody, questionOptions, reasonWords, scopeLabel, scopesFor, signedInBody, subjectWords } from "./conversationAskModel.mjs";
import { Button, Chip, FoldedText, ICON, TextArea, useToast } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

export function AskCard({ ask, conversationId, onAnswered }: { ask: AskView; conversationId: string; onAnswered: () => void }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [denying, setDenying] = useState(false);
  const [note, setNote] = useState("");
  /** The words typed to a question. */
  const [reply, setReply] = useState("");
  const words = subjectWords(ask);
  // The two subjects the desk gained are hand-typed until the generator runs.
  const subject = ask.subject as AskView["subject"] | AskSubjectAsked;
  const content = subject.kind === "content" ? subject : null;
  const question = subject.kind === "question";
  const signIn = subject.kind === "auth" ? subject : null;
  const options = questionOptions(ask);
  const typed = answerBody(reply);

  const answer = async (body: AnswerAskBody) => {
    if (busy) return;
    setBusy(true);
    try {
      await api.answerAsk(conversationId, ask.id, body);
      onAnswered();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : t("studio-ask-card-could-not-answer"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-3 mb-2 flex flex-col gap-2 rounded-card border border-accent/40 bg-accent-soft/30 p-3 text-xs">
      <div className="flex items-center gap-2">
        {content && <ICON.guard size={12} aria-hidden className="shrink-0 text-accent-ink" />}
        <Chip tone="accent">{words.chip}</Chip>
        <span className="font-semibold text-text">{words.name}</span>
        <span className="ml-auto text-2xs text-text-dim">{ask.agent}</span>
      </div>
      {content ? (
        <>
          {/* Where it came from, why it was held, and what it says — the
              screen's account, so the person decides knowing the risk. */}
          <p className="text-sm text-text">{ask.question}</p>
          {content.url && (
            <p className="truncate font-mono text-2xs text-text-dim" title={content.url}>
              {content.url}
            </p>
          )}
          <p className="text-2xs text-warn">{reasonWords(ask)}</p>
          {content.excerpt && (
            // The card draws the edge; what it quotes sits on the surface's own ground, unboxed.
            <div className="rounded-control bg-surface px-2 py-1.5 text-2xs text-text-dim">
              <FoldedText text={content.excerpt} />
            </div>
          )}
        </>
      ) : question || signIn ? (
        <>
          {/* The agent's own words — a question, or that it needs a sign-in; a URL is text, never a door. */}
          <p className="text-sm text-text">{ask.question}</p>
          {signIn?.url && (
            <p className="truncate font-mono text-2xs text-text-dim" title={signIn.url}>
              {signIn.url}
            </p>
          )}
        </>
      ) : (
        <pre className="whitespace-pre-wrap rounded-control bg-surface px-2 py-1.5 font-mono text-2xs text-text">{ask.question}</pre>
      )}
      {denying ? (
        <div className="flex flex-col gap-1.5">
          <TextArea rows={2} value={note} aria-label={t("studio-ask-card-note-agent-optional")} placeholder={t("studio-ask-card-note-agent-optional")} disabled={busy} onChange={(e) => setNote(e.target.value)} />
          <div className="flex gap-2">
            <Button variant="danger" disabled={busy} onClick={() => void answer(denyBody(note))}>
              {busy ? "…" : question ? t("studio-ask-card-decline") : signIn ? t("studio-ask-card-not-now") : t("studio-ask-card-deny")}
            </Button>
            <Button variant="ghost" disabled={busy} onClick={() => setDenying(false)}>{t("studio-ask-card-back")}</Button>
          </div>
        </div>
      ) : question ? (
        <div className="flex flex-col gap-1.5">
          {options.length > 0 && (
            // A choice the agent offered is one click; the words below take anything else.
            <div className="flex flex-wrap gap-2">
              {options.map((option) => (
                <Button key={option} variant="ghost" disabled={busy} onClick={() => void answer({ answer: "answer", text: option })}>
                  {option}
                </Button>
              ))}
            </div>
          )}
          <TextArea rows={2} value={reply} aria-label={t("studio-ask-card-your-answer")} placeholder={t("studio-ask-card-your-answer")} disabled={busy} onChange={(e) => setReply(e.target.value)} />
          <div className="flex gap-2">
            <Button variant="primary" disabled={busy || typed === null} disabledReason={typed === null ? t("studio-ask-card-words-first") : undefined} onClick={() => typed && void answer(typed)}>
              {t("studio-ask-card-answer")}
            </Button>
            <Button variant="danger" disabled={busy} onClick={() => setDenying(true)}>{t("studio-ask-card-decline")}</Button>
          </div>
        </div>
      ) : signIn ? (
        <div className="flex flex-wrap gap-2">
          <Button variant="primary" disabled={busy} onClick={() => void answer(signedInBody())}>{t("studio-ask-card-signed-in")}</Button>
          <Button variant="danger" disabled={busy} onClick={() => setDenying(true)}>{t("studio-ask-card-not-now")}</Button>
        </div>
      ) : (
        <div className="flex flex-wrap gap-2">
          {scopesFor(ask).map((scope) => (
            <Button key={scope} variant={scope === "once" ? "primary" : "ghost"} disabled={busy} onClick={() => void answer(allowBody(ask, scope))}>
              {scopeLabel(scope, ask)}
            </Button>
          ))}
          <Button variant="danger" disabled={busy} onClick={() => setDenying(true)}>{t("studio-ask-card-deny")}</Button>
        </div>
      )}
    </div>
  );
}

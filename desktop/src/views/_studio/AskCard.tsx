/**
 * An open ask at the foot of a conversation's timeline (ide/09, 11 —
 * Security). Two things are asked about: a tool call the Tool & Commands
 * Guard put to a person rather than deciding — a call no guard rule decided
 * in manual or auto, or any command in plan — and content from outside the
 * content screen held before the agent read it: a page or a review, with
 * its source, the reason (the classifier's sentence, or no verdict) and an
 * excerpt, so the person sees the risk in the agent's own place. **Allow
 * once** and **Allow for this conversation** / **Allow this site for this
 * conversation** (only when `grantable`) let it through; **Deny**, with an
 * optional note, refuses it and the agent goes on with a sentence.
 *
 * Driven by `ask_opened` / `ask_settled` plus one `GET` on mount
 * (`useConversationAsks`, `_workbench/reviewStore` for the changes side —
 * this card keeps its own small read since an ask is not a change). A URL is
 * shown as text, never navigated (ide/17).
 */

import { useState } from "react";
import { api } from "../../api";
import type { AskView } from "../../types";
import { allowBody, denyBody, reasonWords, scopeLabel, scopesFor, subjectWords } from "./conversationAskModel.mjs";
import { Button, Chip, FoldedText, ICON, TextArea, useToast } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

export function AskCard({ ask, conversationId, onAnswered }: { ask: AskView; conversationId: string; onAnswered: () => void }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [denying, setDenying] = useState(false);
  const [note, setNote] = useState("");
  const words = subjectWords(ask);
  const content = ask.subject.kind === "content" ? ask.subject : null;

  const answer = async (body: ReturnType<typeof allowBody> | ReturnType<typeof denyBody>) => {
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
    <div className="mx-3 mb-2 flex flex-col gap-2 rounded-card border border-warn/40 bg-warn-soft/30 p-3 text-xs">
      <div className="flex items-center gap-2">
        {content && <ICON.guard size={12} aria-hidden className="shrink-0 text-warn" />}
        <Chip tone="warn">{words.chip}</Chip>
        <span className="font-semibold text-text">{words.name}</span>
        <span className="ml-auto text-2xs text-text-dim">{ask.agent}</span>
      </div>
      {content ? (
        <>
          {/* Where it came from, why it was held, and what it says — the
              screen's account, so the person decides knowing the risk. */}
          <p className="text-2xs text-text">{ask.question}</p>
          {content.url && (
            <p className="truncate font-mono text-2xs text-text-dim" title={content.url}>
              {content.url}
            </p>
          )}
          <p className="text-2xs text-warn">{reasonWords(ask)}</p>
          {content.excerpt && (
            <div className="rounded-control border border-border bg-surface px-2 py-1.5 text-2xs text-text-dim">
              <FoldedText text={content.excerpt} />
            </div>
          )}
        </>
      ) : (
        <pre className="whitespace-pre-wrap rounded-control border border-border bg-surface px-2 py-1.5 font-mono text-2xs text-text">{ask.question}</pre>
      )}
      {denying ? (
        <div className="flex flex-col gap-1.5">
          <TextArea rows={2} value={note} placeholder={t("studio-ask-card-note-agent-optional")} disabled={busy} onChange={(e) => setNote(e.target.value)} />
          <div className="flex gap-2">
            <Button variant="danger" disabled={busy} onClick={() => void answer(denyBody(note))}>
              {busy ? "…" : t("studio-ask-card-deny")}
            </Button>
            <Button variant="ghost" disabled={busy} onClick={() => setDenying(false)}>{t("studio-ask-card-back")}</Button>
          </div>
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

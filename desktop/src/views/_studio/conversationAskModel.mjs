/**
 * An open ask in a conversation about a checkout (ide/09, ide/11 —
 * Security): the Tool & Commands Guard put a tool call to a person rather
 * than deciding it — a call no guard rule decided, in manual or auto, or any
 * command in plan — or the content screen held what an agent was about to
 * read from outside: a page, a review, with its source, the reason and an
 * excerpt. Answered with **Allow once**, **Allow for this conversation** /
 * **Allow this site for this conversation** (only when the ask is
 * `grantable`) or **Deny**, with an optional note.
 *
 * This is not `askModel.mjs`: that module is the goal's decision/answer
 * gates (`api.decide`), a person answering a question an agent asked in
 * words. This one is a tool call waiting on a verdict
 * (`api.answerAsk`) — a different protocol, a different shape, and it is
 * never the agent's own to settle (no MCP tool answers it).
 *
 * Plain `.mjs` with a `.d.mts` beside it.
 */

import { t as tr } from "../../i18n/l10n.mjs";

/** The word a tier reads as, and the sentence naming what a call like it can do. */
export function tierWords(tier) {
  switch (tier) {
    case "write":
      return tr("studio-conversation-ask-tier-write");
    case "exec":
      return tr("studio-conversation-ask-tier-exec");
    default:
      return tr("studio-conversation-ask-tier-read");
  }
}

/** Every scope offered, in order — `"conversation"` only when the ask allows it. */
export function scopesFor(ask) {
  const scopes = ["once"];
  if (ask?.grantable) scopes.push("conversation");
  return scopes;
}

/** Whether an ask is about content from outside rather than a tool call. */
export function isContentAsk(ask) {
  return ask?.subject?.kind === "content";
}

/** The word a scope reads as on its button — a site's, for content. */
export function scopeLabel(scope, ask = null) {
  if (scope !== "conversation") return tr("studio-conversation-ask-allow-once");
  return isContentAsk(ask) ? tr("studio-conversation-ask-allow-site-conversation") : tr("studio-conversation-ask-allow-conversation");
}

/**
 * The ask's header: what it is about, in words — *a command · Bash*, or
 * *content · example.com* — and the chip's word.
 * @param {{subject?: {kind: string, tool?: string, tier?: string, source?: string}} | null | undefined} ask
 */
export function subjectWords(ask) {
  const s = ask?.subject;
  if (!s) return { chip: tierWords(null), name: "", kind: "tool" };
  if (s.kind === "content") return { chip: tr("studio-conversation-ask-content"), name: s.source ?? "", kind: "content" };
  return { chip: tierWords(s.tier), name: s.tool ?? "", kind: "tool" };
}

/**
 * Why a content was held, in a sentence for the card: the screen's reason
 * as the node said it — the classifier's word, or that it gave none — drawn
 * as it came, never matched as English. Empty for a tool ask.
 * @param {{subject?: {kind: string, reason?: string}} | null | undefined} ask
 */
export function reasonWords(ask) {
  const reason = ask?.subject?.kind === "content" ? String(ask.subject.reason ?? "").trim() : "";
  if (!reason) return "";
  return tr("studio-conversation-ask-held", { reason });
}

/** The body `api.answerAsk` sends for an allow, at the chosen scope. */
export function allowBody(ask, scope) {
  const s = scope === "conversation" && ask?.grantable ? "conversation" : "once";
  return { answer: "allow", scope: s };
}

/** The body `api.answerAsk` sends for a deny, with its optional note. */
export function denyBody(note) {
  const t = typeof note === "string" ? note.trim() : "";
  return t ? { answer: "deny", note: t } : { answer: "deny" };
}

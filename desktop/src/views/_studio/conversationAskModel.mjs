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
 * In a conversation that has no goal to escalate to — about a checkout, a
 * workflow, the workspace or the node — the agent's own **question** in
 * words and a **sign-in** it needs are asked on the same desk: a question is
 * answered with the words typed or one of the choices offered (`answerBody`)
 * or declined; a sign-in is *Signed in* (`signedInBody`, an allow once —
 * never for the conversation) or *Not now*.
 *
 * This is not `askModel.mjs`: that module is the goal's decision/answer
 * gates (`api.decide`), where a question goes when the conversation is
 * about a goal. This one is the desk's (`api.answerAsk`) — a different
 * protocol, a different shape, and never the agent's own to settle (no MCP
 * tool answers it).
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

/** Whether an ask is the agent's own question in words. */
export function isQuestionAsk(ask) {
  return ask?.subject?.kind === "question";
}

/** Whether an ask is a sign-in the agent needs. */
export function isAuthAsk(ask) {
  return ask?.subject?.kind === "auth";
}

/**
 * The choices a question offers, each a non-empty word; none for any other
 * ask, or for a question asked open.
 * @param {{subject?: {kind: string, options?: unknown}} | null | undefined} ask
 * @returns {string[]}
 */
export function questionOptions(ask) {
  const options = isQuestionAsk(ask) ? ask.subject.options : null;
  return Array.isArray(options) ? options.filter((o) => typeof o === "string" && o.trim()).map((o) => o.trim()) : [];
}

/**
 * The body `api.answerAsk` sends for a question's answer — the words, trimmed;
 * `null` for nothing to say, which the card never sends.
 * @param {unknown} text
 */
export function answerBody(text) {
  const words = typeof text === "string" ? text.trim() : "";
  return words ? { answer: "answer", text: words } : null;
}

/** The body that says a sign-in is done: an allow, once — a sign-in is never granted for a conversation. */
export function signedInBody() {
  return { answer: "allow", scope: "once" };
}

/** The word a scope reads as on its button — a site's, for content. */
export function scopeLabel(scope, ask = null) {
  if (scope !== "conversation") return tr("studio-conversation-ask-allow-once");
  return isContentAsk(ask) ? tr("studio-conversation-ask-allow-site-conversation") : tr("studio-conversation-ask-allow-conversation");
}

/**
 * The ask's header: what it is about, in words — *a command · Bash*,
 * *content · example.com*, *a question*, *a sign-in · github* — and the
 * chip's word.
 * @param {{subject?: {kind: string, tool?: string, tier?: string, source?: string, provider?: string}} | null | undefined} ask
 */
export function subjectWords(ask) {
  const s = ask?.subject;
  if (!s) return { chip: tierWords(null), name: "", kind: "tool" };
  if (s.kind === "content") return { chip: tr("studio-conversation-ask-content"), name: s.source ?? "", kind: "content" };
  if (s.kind === "question") return { chip: tr("studio-conversation-ask-question"), name: "", kind: "question" };
  if (s.kind === "auth") return { chip: tr("studio-conversation-ask-sign-in"), name: s.provider ?? "", kind: "auth" };
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

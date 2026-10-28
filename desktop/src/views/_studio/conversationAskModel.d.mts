/**
 * Types for `conversationAskModel.mjs`. This file is the only reason
 * TypeScript never has to read it.
 */

import type { AnswerAskBody, AskView } from "../../types";

export type AskScope = "once" | "conversation";

export declare function tierWords(tier: string | null | undefined): string;
export declare function scopesFor(ask: Pick<AskView, "grantable"> | null | undefined): AskScope[];
export declare function isContentAsk(ask: Pick<AskView, "subject"> | null | undefined): boolean;
/** The agent's own question in words. */
export declare function isQuestionAsk(ask: Pick<AskView, "subject"> | null | undefined): boolean;
/** A sign-in the agent needs. */
export declare function isAuthAsk(ask: Pick<AskView, "subject"> | null | undefined): boolean;
/** The choices a question offers; none for any other ask. */
export declare function questionOptions(ask: Pick<AskView, "subject"> | null | undefined): string[];
/** The words of an answer to a question, trimmed; `null` for nothing to say. */
export declare function answerBody(text: unknown): AnswerAskBody | null;
/** A sign-in done: an allow, once. */
export declare function signedInBody(): AnswerAskBody;
export declare function scopeLabel(scope: AskScope, ask?: Pick<AskView, "subject"> | null): string;
export declare function subjectWords(ask: Pick<AskView, "subject"> | null | undefined): { chip: string; name: string; kind: "tool" | "content" | "question" | "auth" };
export declare function reasonWords(ask: Pick<AskView, "subject"> | null | undefined): string;
export declare function allowBody(ask: Pick<AskView, "grantable"> | null | undefined, scope: AskScope): AnswerAskBody;
export declare function denyBody(note?: string | null): AnswerAskBody;

/**
 * Types for `conversationAskModel.mjs`. This file is the only reason
 * TypeScript never has to read it.
 */

import type { AnswerAskBody, AskView } from "../../types";

export type AskScope = "once" | "conversation";

export declare function tierWords(tier: string | null | undefined): string;
export declare function scopesFor(ask: Pick<AskView, "grantable"> | null | undefined): AskScope[];
export declare function isContentAsk(ask: Pick<AskView, "subject"> | null | undefined): boolean;
export declare function scopeLabel(scope: AskScope, ask?: Pick<AskView, "subject"> | null): string;
export declare function subjectWords(ask: Pick<AskView, "subject"> | null | undefined): { chip: string; name: string; kind: "tool" | "content" };
export declare function reasonWords(ask: Pick<AskView, "subject"> | null | undefined): string;
export declare function allowBody(ask: Pick<AskView, "grantable"> | null | undefined, scope: AskScope): AnswerAskBody;
export declare function denyBody(note?: string | null): AnswerAskBody;

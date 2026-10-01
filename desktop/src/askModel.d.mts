/**
 * Types for `askModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 *
 * `AskKind`, `AskOption` and `Answer` come from the generated schema rather
 * than being restated here: the whole point of the module is that one place
 * knows the protocol, and a second hand-written copy of it is the drift it
 * exists to prevent.
 */

import type { Answer, AskKind, AskOption } from "./types";

/** Everything the module needs off a pending ask. `NeedsAction` widens to it. */
export interface PendingAsk {
  gate_id?: string | null;
  expects: AskKind;
}

/** What the form holds while it is being filled in. */
export interface AskForm {
  /** Chosen option ids, in the order they were picked. */
  selected: string[];
  /** The free-text box — an answer beside a question, a rationale beside a gate. */
  text: string;
  /** Set only by the "I'm not sure" path. */
  unsure?: boolean;
  /** A decision's verdict. Ignored for a question, which never declines. */
  approve?: boolean;
  /** Run inputs for an adoption gate; ignored on a question and on a decline. */
  inputs?: Record<string, unknown>;
}

/** An option with its blanks filled in and at most one recommendation left. */
export interface NormalizedOption {
  id: string;
  label: string;
  detail: string | null;
  recommended: boolean;
}

/** The shape `api.decide` takes. */
export interface DecideRequest {
  approve: boolean;
  gate?: string;
  rationale?: string;
  answer?: Answer;
  /** Run inputs, sent only with an approval — an adoption starts the run with them. */
  inputs?: Record<string, unknown>;
}

export declare function isAnswerAsk(expects: AskKind | null | undefined): boolean;
export declare function askOptions(expects: AskKind | null | undefined): NormalizedOption[];
export declare function isMulti(expects: AskKind | null | undefined): boolean;
export declare function toggleChoice(
  selected: readonly string[] | null | undefined,
  id: string,
  multi: boolean,
): string[];
export declare function keepOffered(
  expects: AskKind | null | undefined,
  selected: readonly string[] | null | undefined,
): string[];
export declare function canSubmit(action: PendingAsk, form: AskForm): boolean;
export declare function decideBody(action: PendingAsk, form: AskForm): DecideRequest | null;
export declare function answerSummary(answer: Answer | null | undefined): string | null;

export type { Answer, AskKind, AskOption };

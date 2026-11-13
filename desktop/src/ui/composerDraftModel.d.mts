/** Types for `composerDraftModel.mjs`. */
export declare const DRAFT_PREFIX: string;
export declare function draftKey(scope: string): string;
export declare function afterSend(current: string, sent: string): string;
export declare function draftWhileSending(): null;
export declare function draftAfterFailure(current: string | null | undefined, sent: string): string | null;

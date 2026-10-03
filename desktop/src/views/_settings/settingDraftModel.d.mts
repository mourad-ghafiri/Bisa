/** Types for `settingDraftModel.mjs`. */

/** How long the quiet *Saved* beside a row stays after a write lands, in ms. */
export declare const SAVED_NOTE_MS: number;
/** The box's text for a value. */
export declare function draftOf(value: unknown): string;
/** The value a draft commits, or `undefined` when it commits nothing. */
export declare function committedValue(
  kind: { type: string; min?: number; max?: number },
  draft: string,
  current: unknown,
): string | number | undefined;

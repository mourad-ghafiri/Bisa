/** Types for `openedModel.mjs`, plain JavaScript so `node --test` reads it. */

export declare const MAX_OPENED: number;
/** The opened rows read back from a screen's memory, or `undefined` for what is no list of rows. */
export declare function parseOpened(raw: unknown): ReadonlySet<string> | undefined;
export declare function toggled(opened: ReadonlySet<string>, key: string, cap?: number): ReadonlySet<string>;

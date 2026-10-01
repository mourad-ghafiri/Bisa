/**
 * Types for `railLayoutModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

export declare const INDENT_PX: number;
export declare const INDENT_BASE_PX: number;
export declare const TWISTY_PX: number;

export declare function guideInset(): number;
export declare function rowHeightToken(kind: string): "--spacing-row" | "--spacing-row-sm";

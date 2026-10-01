/** Types for `patchViewModel.mjs`, plain JavaScript so `node --test` reads it. */

export type PatchView = "hunks" | "split" | "inline";

export interface PatchSides {
  original?: string | null;
  modified?: string | null;
  binary: boolean;
  truncated: boolean;
}

export declare const PATCH_VIEWS: readonly PatchView[];
export declare const PATCH_VIEW_LABEL: Readonly<Record<PatchView, string>>;
export declare function patchViewGlyph(view: PatchView): "list" | "splitRight" | "splitDown";
export declare function patchViewHint(view: PatchView, facts?: { readOnly?: boolean }): string;
export declare function needsSides(view: string): boolean;
export declare function sidesWords(sides: PatchSides | null | undefined): string | null;

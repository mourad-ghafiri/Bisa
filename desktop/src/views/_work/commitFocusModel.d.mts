/** Types for `commitFocusModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { PatchView } from "./patchViewModel.mjs";

export interface CommitFile {
  path: string;
  old_path?: string | null;
  kind: string;
}

export type CommitFocus = { kind: "all" } | { kind: "file"; path: string; oldPath: string | null; change: string };

export declare function focusOf(files: readonly CommitFile[], chosen: string | null, view: PatchView): CommitFocus | null;
export declare function chooseFile(chosen: string | null, path: string, view: PatchView): string | null;
export declare function isFocused(focus: CommitFocus | null, path: string): boolean;
export declare function emptyPatchWords(focus: CommitFocus | null): string;
export declare function listHint(view: PatchView): string;
/** The line over a commit's patch the node cut where its read stops. */
export declare function cutWords(): string;

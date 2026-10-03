/** Types for `designDraftModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { NewWorkflowBody } from "../../types";

/** How many goals' designs the window holds with their undo history. */
export declare const MAX_DRAFTS: number;
/** A drawing read back from the memory, or `null` for what is no drawing. */
export declare function draftBody(raw: unknown): NewWorkflowBody | null;
export declare function draftChanged(present: object | null | undefined, stored: object | null | undefined, undoable: boolean): boolean;

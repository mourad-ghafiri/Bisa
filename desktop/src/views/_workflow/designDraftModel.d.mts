/** Types for `designDraftModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { NewWorkflowBody } from "../../types";

/** How many goals' designs the window holds with their undo history. */
export declare const MAX_DRAFTS: number;
/** A drawing read back from the memory, or `null` for what is no drawing. */
export declare function draftBody(raw: unknown): NewWorkflowBody | null;
export declare function draftChanged(present: object | null | undefined, stored: object | null | undefined, undoable: boolean): boolean;
/** Whether *Save changes* may send: a whole, judged drawing and no save in flight. */
export declare function canSave(state: { draft?: object | null; problems?: readonly unknown[] | null; validating?: boolean; error?: string | null; saving?: boolean }): boolean;
/** The drawing after its save landed: nothing when the head is what was sent, else the history as it stands. */
export declare function afterSaved<H extends { present: unknown }>(history: H | null | undefined, sent: unknown): H | null;

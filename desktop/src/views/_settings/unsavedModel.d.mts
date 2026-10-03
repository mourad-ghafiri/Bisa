/** Types for `unsavedModel.mjs`. */

/** The set of forms holding unsaved edits, with `id` marked dirty or clean. */
export declare function markForm(forms: ReadonlySet<string>, id: string, dirty: boolean): ReadonlySet<string>;
/** Whether going from panel `from` to panel `to` asks first. */
export declare function leaveAsks(forms: ReadonlySet<string>, from: string, to: string): boolean;

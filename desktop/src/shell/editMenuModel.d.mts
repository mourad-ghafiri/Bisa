/** Types for `editMenuModel.mjs`, plain JavaScript so `node --test` reads it. */

export type EditVerb = "undo" | "redo" | "cut" | "copy" | "paste" | "select_all";

export declare const EDIT_VERBS: readonly EditVerb[];
/** The verbs a focused code editor is asked first: Undo, Redo, Select All. */
export declare const HANDED_TO_EDITOR: readonly EditVerb[];
export declare const EDIT_VERB_EVENT: string;
export declare function editVerbId(verb: EditVerb): string;
export declare function isEditVerb(word: unknown): word is EditVerb;
export declare function handedToEditor(verb: string): boolean;
export declare function replaysChord(verb: EditVerb, takenByEditor: boolean): boolean;
export declare function keyOfVerb(verb: EditVerb): "x" | "c" | "v" | "a" | "z";
/** Whether the verb's chord holds Shift — ⌘⇧Z, redo's. */
export declare function shiftOfVerb(verb: string): boolean;

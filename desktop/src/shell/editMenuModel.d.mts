/** Types for `editMenuModel.mjs`, plain JavaScript so `node --test` reads it. */

export type EditVerb = "cut" | "copy" | "paste" | "select_all";

export declare const EDIT_VERBS: readonly EditVerb[];
export declare const EDIT_VERB_EVENT: string;
export declare function editVerbId(verb: EditVerb): string;
export declare function isEditVerb(word: unknown): word is EditVerb;
export declare function replaysChord(verb: EditVerb, takenByEditor: boolean): boolean;
export declare function keyOfVerb(verb: EditVerb): "x" | "c" | "v" | "a";

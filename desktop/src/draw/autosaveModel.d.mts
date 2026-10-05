/** Types for `autosaveModel.mjs`. */
export interface Autosave {
  inFlight: boolean;
  again: boolean;
  conflicted: boolean;
  dirty: boolean;
  savedHash: string;
  /** The hash of a frame heard while a save was in the air, until that save answers. */
  heard: string | null;
  /** A reload the canvas could not perform yet — not loaded — done at its first change. */
  owed: boolean;
}
export declare function opened(hash: string): Autosave;
/** The canvas at the hash the store last gave through it, whoever saved; `null` leaves the hash it opened at. */
export declare function atStore(s: Autosave, storeHash: string | null | undefined): Autosave;
/** A reload owed to the canvas's first change, since it could not be performed now. */
export declare function reloadOwed(s: Autosave): Autosave;
export declare function changed(s: Autosave): Autosave;
export declare function saveAsked(s: Autosave): { run: boolean; next: Autosave };
export declare function saveLanded(s: Autosave, hash: string): Autosave;
export declare function saveConflicted(s: Autosave): Autosave;
export declare function saveFailed(s: Autosave): Autosave;
export declare function adopted(s: Autosave, hash: string): Autosave;
export declare function abandoned(s: Autosave): Autosave;
/** What a `drawing_changed` frame means for the open canvas — `wait` while a save is in the air. */
export declare function reloadDecision(s: Autosave, incomingHash: string): "ignore" | "wait" | "reload" | "conflict";
/** A frame heard while a save was in the air, kept until it answers. */
export declare function frameHeard(s: Autosave, hash: string): Autosave;
/** The frame heard during a save, judged once the save answered. */
export declare function heardAfterSave(s: Autosave): { decision: "ignore" | "reload" | "conflict"; next: Autosave };

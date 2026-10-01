/**
 * How the canvas saves (19 — Drawings), as a state machine with no React in
 * it: one PATCH in flight at a time, a stroke during a save re-arms one
 * after it lands, a 409 freezes saving until the person adopts what is
 * there, and a `drawing_changed` frame from elsewhere is a reload when the
 * canvas is clean and a conflict when it is not. Lifted from the note
 * editor's live-ref loop so the rules can be read and tested here.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

/**
 * @typedef {{
 *   inFlight: boolean,
 *   again: boolean,
 *   conflicted: boolean,
 *   dirty: boolean,
 *   savedHash: string,
 *   heard: string | null,
 * }} Autosave
 */

/** A canvas that just opened on a drawing: clean, at the hash it read, with no frame waiting to be judged. @param {string} hash */
export function opened(hash) {
  return { inFlight: false, again: false, conflicted: false, dirty: false, savedHash: hash, heard: null };
}

/** A stroke landed: the scene is dirty. A conflicted canvas stays conflicted. @param {Autosave} s */
export function changed(s) {
  if (s.dirty) return s;
  return { ...s, dirty: true };
}

/**
 * Whether the timer's save should run now: something to save, no conflict,
 * no save in the air. A save asked while one is in the air is remembered
 * (`again`) and run when it lands.
 * @param {Autosave} s
 * @returns {{run: boolean, next: Autosave}}
 */
export function saveAsked(s) {
  if (s.conflicted || !s.dirty) return { run: false, next: s };
  if (s.inFlight) return { run: false, next: s.again ? s : { ...s, again: true } };
  return { run: true, next: { ...s, inFlight: true, dirty: false } };
}

/** The PATCH landed with the store's new hash. @param {Autosave} s @param {string} hash */
export function saveLanded(s, hash) {
  return { ...s, inFlight: false, savedHash: hash, again: false, dirty: s.dirty || s.again };
}

/** The PATCH was refused with a 409: saving stops until the person adopts theirs — and a frame heard meanwhile has nothing more to say. @param {Autosave} s */
export function saveConflicted(s) {
  return { ...s, inFlight: false, again: false, conflicted: true, heard: null };
}

/** The PATCH failed for another reason: the strokes stay dirty and the next timer tries again. @param {Autosave} s */
export function saveFailed(s) {
  return { ...s, inFlight: false, again: false, dirty: true };
}

/** The person adopted what is there: clean again, at that hash. @param {Autosave} s @param {string} hash */
export function adopted(s, hash) {
  return opened(hash);
}

/**
 * The canvas is let go of — *Don't save*, or the drawing is being deleted:
 * nothing is dirty any more and nothing more is asked of it, so a flush on
 * the way out and a late stroke both do nothing.
 * @param {Autosave} s
 */
export function abandoned(s) {
  return { ...s, dirty: false, again: false, conflicted: true };
}

/**
 * What a `drawing_changed` frame for the open drawing means: nothing when it
 * carries the hash this canvas last saved (its own write, echoed); **wait**
 * while a save is in the air — the bus and the PATCH's answer race, so the
 * frame may be that very save's echo arriving first, and only the answer can
 * tell (`frameHeard`, `heardAfterSave`); a reload when the canvas is clean;
 * a conflict when strokes are unsaved.
 * @param {Autosave} s
 * @param {string} incomingHash
 * @returns {"ignore" | "wait" | "reload" | "conflict"}
 */
export function reloadDecision(s, incomingHash) {
  if (incomingHash === s.savedHash) return "ignore";
  if (s.inFlight) return "wait";
  if (s.dirty || s.again) return "conflict";
  return "reload";
}

/** A frame heard while a save was in the air: kept — the latest one — until the save answers. @param {Autosave} s @param {string} hash */
export function frameHeard(s, hash) {
  return { ...s, heard: hash };
}

/**
 * The save answered — it landed, or it failed: the frame heard while it was
 * in the air is judged now, against the hash the canvas stands at. The
 * save's own echo is nothing; any other hash is somebody else's scene — a
 * reload when the canvas is clean, a conflict when strokes are unsaved. A
 * canvas already frozen, or one that heard nothing, has nothing to judge.
 * @param {Autosave} s
 * @returns {{decision: "ignore" | "reload" | "conflict", next: Autosave}}
 */
export function heardAfterSave(s) {
  if (s.heard === null) return { decision: "ignore", next: s };
  const next = { ...s, heard: null };
  if (s.conflicted) return { decision: "ignore", next };
  const decision = reloadDecision(next, s.heard);
  return { decision: decision === "wait" ? "ignore" : decision, next };
}

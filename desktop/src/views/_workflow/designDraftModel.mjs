/**
 * A design in progress, as it is kept across a restart
 * (`designDraftStore.ts`): the drawing as it stands — its present, never its
 * undo history — read back from a memory that outlives the window, and so
 * may have been written by another version or by hand. What is no drawing
 * is nothing: the tab opens on the stored workflow, as if nothing had been
 * drawn. Whether the drawing is a *valid* workflow is the node's to say, as
 * it is for every drawing (`useLiveValidation`). Facts only, no React. Plain
 * `.mjs`, so `node --test` reads it.
 */

/**
 * How many goals' designs the window holds with their undo history — the
 * newest drawn. A design past it keeps its drawing in the memory and comes
 * back as one read after a restart does: as it stands, a history of one
 * entry.
 */
export const MAX_DRAFTS = 16;

/** Whether a value is an object with fields, not a list. */
const isRecord = (v) => !!v && typeof v === "object" && !Array.isArray(v);

/**
 * A drawing read back from the memory: a body with a name and its steps,
 * each step a record with an id — or `null` for anything else.
 * @param {unknown} raw
 * @returns {object | null} the body, as it was kept
 */
export function draftBody(raw) {
  if (!isRecord(raw)) return null;
  if (typeof raw.name !== "string" || !Array.isArray(raw.steps)) return null;
  if (!raw.steps.every((step) => isRecord(step) && typeof step.id === "string" && step.id.length > 0 && typeof step.kind === "string")) return null;
  if (raw.inputs !== undefined && !Array.isArray(raw.inputs)) return null;
  if (raw.tags !== undefined && !(Array.isArray(raw.tags) && raw.tags.every((tag) => typeof tag === "string"))) return null;
  return raw;
}

/**
 * Whether throwing the drawing away loses anything — what *Discard changes*
 * asks before. An edit made in this window does; so does a drawing read
 * back after a restart that differs from the stored workflow, since its
 * history was not kept and `canUndo` alone would call it untouched. A
 * drawing on a blank canvas is all there is of the design, so it counts.
 * @param {object | null | undefined} present the drawing as it stands
 * @param {object | null | undefined} stored the stored workflow's body, `null` on a blank canvas
 * @param {boolean} undoable whether this window holds an edit to undo
 */
export function draftChanged(present, stored, undoable) {
  if (present == null) return false;
  if (undoable) return true;
  if (stored == null) return true;
  return JSON.stringify(present) !== JSON.stringify(stored);
}

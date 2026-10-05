/**
 * The canvases that are open right now, by drawing (19 — Drawings): the
 * editor registers its imperative API once the canvas has **loaded** the
 * drawing — Excalidraw hands the API over before `initialData` lands and
 * reports no change while it loads, so the first `onChange` is the load and
 * the record is made from it, at the hash the drawing was read at — and the
 * bridge performs an agent's request through it, so the person watches the
 * shapes land. Beside each, **what was last saved** — the hash the store
 * answered and the elements it holds — kept here rather than in the editor,
 * because two writers save through one canvas: the person's autosave and the
 * bridge. Either one's save updates the one record, so a `drawing_changed`
 * frame that echoes it is recognised as ours by whoever hears it, and the
 * editor's next `onChange` compares against the scene the store has.
 *
 * The record is **the open canvas's**: a save made through another canvas —
 * the bridge's offscreen one, a mount that closed while its flush was in the
 * air — moves nothing here, and a close forgets only its own record. An
 * offscreen save therefore reaches an open canvas as somebody else's frame,
 * which the editor adopts when clean, never as a record saying the canvas
 * already holds what it does not — the way an agent's drawing was once
 * overwritten by the autosave of a canvas that had just opened on it.
 */

import type { ExcalidrawImperativeAPI, OrderedExcalidrawElement } from "../ui/excalidraw";

export interface Saved {
  readonly hash: string;
  readonly elements: readonly OrderedExcalidrawElement[];
}

interface Live {
  api: ExcalidrawImperativeAPI;
  saved: Saved;
}

const scenes = new Map<string, Live>();

/** A canvas loaded on a drawing, at what the store holds. */
export function registerLiveScene(drawing: string, api: ExcalidrawImperativeAPI, saved: Saved): void {
  scenes.set(drawing, { api, saved });
}

/** The canvas closed; what was saved is forgotten with it — its own record only, never a canvas that took the drawing meanwhile. */
export function unregisterLiveScene(drawing: string, api: ExcalidrawImperativeAPI): void {
  if (scenes.get(drawing)?.api === api) scenes.delete(drawing);
}

/** The open canvas's API for a drawing, or none. */
export function liveSceneFor(drawing: string): ExcalidrawImperativeAPI | null {
  return scenes.get(drawing)?.api ?? null;
}

/**
 * A save landed through `by` — the person's or the bridge's: the record moves
 * only while `by` is the canvas open on the drawing. A save through another
 * canvas — the offscreen one, a mount that closed meanwhile — moves nothing:
 * the open canvas does not hold what that save wrote, and a record saying so
 * would make its next autosave write the old scene over the new. A drawing
 * with no canvas open has no record to move: the bridge read its hash from
 * the store a moment ago and needs no other.
 */
export function noteSaved(drawing: string, saved: Saved, by: ExcalidrawImperativeAPI): void {
  const live = scenes.get(drawing);
  if (live && live.api === by) scenes.set(drawing, { api: live.api, saved });
}

/** What the store last confirmed for a drawing — the open canvas's record, when it is `of`'s canvas (anyone's when `of` is omitted) — or none. */
export function lastSaved(drawing: string, of?: ExcalidrawImperativeAPI): Saved | null {
  const live = scenes.get(drawing);
  return live && (of === undefined || live.api === of) ? live.saved : null;
}

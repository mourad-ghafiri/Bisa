/**
 * The canvases that are open right now, by drawing (19 — Drawings): the
 * editor registers its imperative API while a drawing is on screen, and
 * the bridge performs an agent's request through it, so the person watches
 * the shapes land. Beside each, **what was last saved** — the hash the store
 * answered and the elements it holds — kept here rather than in the editor,
 * because two writers save through one canvas: the person's autosave and the
 * bridge. Either one's save updates the one record, so a `drawing_changed`
 * frame that echoes it is recognised as ours by whoever hears it, and the
 * editor's next `onChange` compares against the scene the store has.
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

/** A canvas opened on a drawing, at what the store holds. */
export function registerLiveScene(drawing: string, api: ExcalidrawImperativeAPI, saved: Saved): void {
  scenes.set(drawing, { api, saved });
}

/** The canvas closed; what was saved is forgotten with it. */
export function unregisterLiveScene(drawing: string): void {
  scenes.delete(drawing);
}

/** The open canvas's API for a drawing, or none. */
export function liveSceneFor(drawing: string): ExcalidrawImperativeAPI | null {
  return scenes.get(drawing)?.api ?? null;
}

/**
 * A save landed — the person's or the bridge's: the open canvas's record
 * moves. A drawing with no canvas open has no record to move: the bridge
 * read its hash from the store a moment ago and needs no other, and a
 * record made here for it would never be forgotten — one scene kept for
 * every drawing an agent ever drew offscreen.
 */
export function noteSaved(drawing: string, saved: Saved): void {
  const live = scenes.get(drawing);
  if (live) scenes.set(drawing, { api: live.api, saved });
}

/** What the store last confirmed for a drawing, or none. */
export function lastSaved(drawing: string): Saved | null {
  return scenes.get(drawing)?.saved ?? null;
}

/**
 * What a canvas keeps of where it was left — the step it had picked and
 * where it looked — so a designer opened again lands where it was, in this
 * window and after a restart: the screen is keyed by its route (`App.tsx`),
 * and every door back opens a bare `#/workflows/<id>`.
 *
 * Kept through the view memory (`shell/viewMemoryStore`), under the place
 * the canvas is drawn at: a workflow's designer under the workflow's, a
 * goal's Workflow tab and a run's page under their own. Quietly — read when
 * a canvas opens, written as the person picks and pans — so nothing on
 * screen follows the memory while it is written. What makes a kept value
 * safe to open on is `designerMemoryModel.mjs`'s.
 */

import { placeOf, viewState } from "../../shell/viewMemoryStore";
import { keptStep, parseMemory, sameViewport, viewportOf, type DesignerMemory, type Viewport } from "./designerMemoryModel.mjs";

/** The names a canvas's memory is kept under, beside its place's other values. */
const STEP = "canvas:step";
const VIEWPORT = "canvas:viewport";

/** The place a workflow's designer keeps its memory under. */
const designerPlace = (workflow: string): string => placeOf({ name: "workflow", id: workflow });

/** Where the canvas drawn at `place` looked when it was left, or nowhere — it opens fitted. */
export function canvasViewportAt(place: string): Viewport | null {
  return viewportOf(viewState.read(place, VIEWPORT));
}

/** Where the canvas drawn at `place` looks now. A viewport that is no place is not kept. */
export function rememberViewportAt(place: string, viewport: { x: number; y: number; zoom: number }): void {
  const next = viewportOf(viewport);
  if (!next || sameViewport(canvasViewportAt(place), next)) return;
  viewState.keepQuietly(place, VIEWPORT, next);
}

/** What the designer of `workflow` was left on, or nothing. */
export function designerMemoryOf(workflow: string): DesignerMemory {
  const place = designerPlace(workflow);
  return parseMemory({ selected: viewState.read(place, STEP), viewport: viewState.read(place, VIEWPORT) });
}

/** The step picked on the workflow's canvas — `null` when nothing is. */
export function rememberSelectedStep(workflow: string, step: string | null): void {
  viewState.keepQuietly(designerPlace(workflow), STEP, keptStep(step));
}

/** Where the workflow's canvas looks now. */
export function rememberCanvasViewport(workflow: string, viewport: { x: number; y: number; zoom: number }): void {
  rememberViewportAt(designerPlace(workflow), viewport);
}

/**
 * A scrollport kept under one name at `place` is kept under another from
 * now on — what a step renamed in the panel asks, so its page comes back
 * where it was scrolled rather than at the top under a name nobody kept.
 * Spelt as `shell/useViewScroll` spells a scroll's name.
 */
export function moveKeptScroll(place: string, from: string, to: string): void {
  if (from === to) return;
  const at = viewState.read(place, `scroll:${from}`);
  if (at === undefined || at === null) return;
  viewState.keepQuietly(place, `scroll:${to}`, at);
}

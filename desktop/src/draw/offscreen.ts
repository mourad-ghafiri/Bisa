/**
 * The offscreen canvas (19 — Drawings): where the bridge performs an
 * agent's request when the drawing is not open on screen. A canvas needs a
 * laid-out box to measure text in and fonts loaded to measure with, so it
 * is a real `Excalidraw` mounted by `DrawPanel` — once, after the first
 * request that needs it, so the canvas's chunk is never in the boot path —
 * standing off the viewport and hidden with `visibility`, never `display:
 * none`. This store is the door: the bridge asks for the API and waits for
 * the mount; the panel mounts and hands the API in.
 */

import { useSyncExternalStore } from "react";
import type { ExcalidrawImperativeAPI } from "../ui/excalidraw";

let wanted = false;
let api: ExcalidrawImperativeAPI | null = null;
const waiting = new Set<(api: ExcalidrawImperativeAPI) => void>();
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** Whether the panel should mount the offscreen canvas now. */
export function useOffscreenWanted(): boolean {
  return useSyncExternalStore(subscribe, () => wanted, () => wanted);
}

/** The mounted canvas handed its API in; every waiter goes on. */
export function offscreenMounted(next: ExcalidrawImperativeAPI): void {
  api = next;
  for (const w of waiting) w(next);
  waiting.clear();
}

/** The offscreen canvas went away — the panel unmounted. */
export function offscreenGone(): void {
  api = null;
}

/** The offscreen canvas's API, mounting it first when it is not there yet. */
export function offscreenApi(): Promise<ExcalidrawImperativeAPI> {
  if (api) return Promise.resolve(api);
  if (!wanted) {
    wanted = true;
    for (const l of listeners) l();
  }
  return new Promise((resolve) => {
    waiting.add(resolve);
  });
}

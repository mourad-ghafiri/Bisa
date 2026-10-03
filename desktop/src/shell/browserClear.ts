/**
 * The boxes of the main page's floating overlays, for the browser layer to
 * cut around (ide/18, `browserClearModel.mjs`). A browser tab is a native
 * view that paints over every DOM element; the Notes and Draw panels, their
 * docks, the pet and the addon windows each publish where they are painted,
 * and `BrowserPanel.tsx` — the one file that talks to the webviews — sends
 * the ones over a showing tab to the shell, which leaves a hole there: the
 * overlay shows and takes its own clicks, and the page stays live around it.
 *
 * A module store like `ui/dockClearance.ts`: the overlays are mounted at
 * different depths, and nothing here decides where an overlay goes.
 */

import { useEffect, useLayoutEffect, useSyncExternalStore, type RefObject } from "react";
import { clearOf, type ClearBox } from "./browserClearModel.mjs";
import { LAYOUT_CHANGED } from "./layerSlots";
import { browserAvailable } from "../browser/session";
import { isMac } from "../ui";

const clears = new Map<string, ClearBox>();
const listeners = new Set<() => void>();
let version = 0;

function emit(): void {
  version++;
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

function publish(id: string, next: ClearBox | null): void {
  const was = clears.get(id);
  if (next === null) {
    if (was === undefined) return;
    clears.delete(id);
  } else {
    if (was && was.left === next.left && was.top === next.top && was.width === next.width && was.height === next.height && was.radius === next.radius) return;
    clears.set(id, next);
  }
  emit();
}

/** An element's box and corner radius as the layer cuts it; `radius` overrides the element's own. */
function measure(el: HTMLElement, radius?: number): ClearBox | null {
  const r = el.getBoundingClientRect();
  const own = radius ?? (Number.parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0);
  return clearOf({ left: r.left, top: r.top, width: r.width, height: r.height }, own);
}

/**
 * Publish an overlay's painted box while `on`, and withdraw it when it is
 * off or unmounts. Read again after every render (a drag re-renders), on a
 * resize of the element or the window, at the end of a transition, and
 * when the shell's layout moves.
 */
export function useBrowserClear(id: string, ref: RefObject<HTMLElement | null>, on: boolean, radius?: number): void {
  useLayoutEffect(() => {
    const el = ref.current;
    publish(id, on && el ? measure(el, radius) : null);
  });
  useEffect(() => {
    const el = ref.current;
    if (!on || !el) return;
    const read = () => publish(id, measure(el, radius));
    const ro = new ResizeObserver(read);
    ro.observe(el);
    window.addEventListener("resize", read);
    window.addEventListener(LAYOUT_CHANGED, read);
    el.addEventListener("transitionend", read);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", read);
      window.removeEventListener(LAYOUT_CHANGED, read);
      el.removeEventListener("transitionend", read);
    };
  }, [id, ref, on, radius]);
  useEffect(() => () => publish(id, null), [id]);
}

/** The overlays as they stand, outside React, each under its id. */
export function browserClearsNow(): Array<{ id: string; clear: ClearBox }> {
  return [...clears].map(([id, clear]) => ({ id, clear }));
}

/** A number that moves whenever an overlay does — for an effect to depend on. */
export function useBrowserClears(): number {
  return useSyncExternalStore(subscribe, () => version, () => version);
}

/** What the shell last answered: whether it cuts around the overlays (macOS) or cannot. */
let cuts: boolean | null = null;
const cutsListeners = new Set<() => void>();

/** The shell's answer to a cut, kept for whoever asks next (`BrowserPanel.tsx`). */
export function noteBrowserCuts(answer: boolean): void {
  if (cuts === answer) return;
  cuts = answer;
  for (const l of cutsListeners) l();
}

function subscribeCuts(l: () => void): () => void {
  cutsListeners.add(l);
  return () => cutsListeners.delete(l);
}

/** `browserCutsAround()`, re-read only when the shell's answer changes — never on an overlay's move. */
export function useBrowserCutsAround(): boolean {
  return useSyncExternalStore(subscribeCuts, browserCutsAround, browserCutsAround);
}

/**
 * Whether the browser layer leaves a hole for an overlay — the shell's own
 * answer once it has given one, until then the platform's (macOS cuts). An
 * overlay that is cut around never has to hide from a page.
 */
function browserCutsAround(): boolean {
  return cuts ?? (isMac && browserAvailable());
}

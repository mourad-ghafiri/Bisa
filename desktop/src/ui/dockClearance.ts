/**
 * Where the floating docks stand, for what must not sit under them.
 *
 * The notes and draw docks float over the content at a placement the person
 * chose (`dockModel.mjs`), the bottom right by default — over the end of a
 * list and over a composer's Send. They are never moved to make room:
 * the placement is the person's. Instead each dock says where it is painted,
 * and the content makes room around it, only while a dock is actually there:
 *
 * - `useDockFootprint` — a dock publishes its painted box, and withdraws it
 *   when it is hidden or unmounts.
 * - `useDockClearance` — the shell's content column learns how far a dock in
 *   its lower half reaches up from its bottom edge, and stamps it as
 *   `--dock-clear-bottom` with a `data-dock-clear` flag; `styles.css` turns it
 *   into room at the end of every screen's scroll region, so its last row can
 *   be scrolled clear of the dock.
 * - `useDockOverlap` — an element pinned at the bottom (a composer) learns how
 *   far a dock covers its right end, so its own controls stay reachable.
 *
 * Geometry only: nothing here decides where a dock goes or whether it shows.
 */

import { useEffect, useLayoutEffect, useState, useSyncExternalStore, type RefObject } from "react";
import type { Box } from "./dockModel.mjs";

/** The air kept between a dock and what it would otherwise cover. */
const GAP = 8;

type Footprint = { left: number; top: number; size: number };

const footprints = new Map<string, Footprint>();
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

/** A dock's painted box, published while it is mounted. */
export function useDockFootprint(id: string, box: Box, size: number): void {
  useEffect(() => {
    footprints.set(id, { left: box.left, top: box.top, size });
    emit();
  }, [id, box.left, box.top, size]);
  useEffect(
    () => () => {
      footprints.delete(id);
      emit();
    },
    [id],
  );
}

/** Re-read on any dock move, any resize of the element, and the window's. */
function useMeasured<T>(ref: RefObject<HTMLElement | null>, measure: (el: HTMLElement) => T, initial: T): T {
  const docks = useSyncExternalStore(subscribe, () => version);
  const [value, setValue] = useState<T>(initial);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const read = () => setValue(measure(el));
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    window.addEventListener("resize", read);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", read);
    };
    // `measure` is a module-level function at every call site.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ref, docks]);
  return value;
}

/** How far a dock in the lower half of `el` reaches up from its bottom edge. */
function bottomReach(el: HTMLElement): number {
  const r = el.getBoundingClientRect();
  let reach = 0;
  for (const d of footprints.values()) {
    const across = d.left < r.right && d.left + d.size > r.left;
    const low = d.top + d.size > r.top + r.height / 2 && d.top < r.bottom;
    if (across && low) reach = Math.max(reach, r.bottom - d.top + GAP);
  }
  return Math.max(0, Math.round(reach));
}

/**
 * How far a dock covering `el`'s row reaches in from its right edge. Measured
 * from where the edge would be without the room already kept (its right
 * margin), so keeping the room never reads as the dock having gone.
 */
function rightReach(el: HTMLElement): number {
  const r = el.getBoundingClientRect();
  const right = r.right + (parseFloat(getComputedStyle(el).marginRight) || 0);
  let reach = 0;
  for (const d of footprints.values()) {
    const along = d.top < r.bottom && d.top + d.size > r.top;
    const over = d.left < right && d.left + d.size > r.left;
    if (along && over) reach = Math.max(reach, right - d.left + GAP);
  }
  return Math.max(0, Math.round(reach));
}

/**
 * Stamps the content column with the room its scroll regions keep at their
 * end. The flag is set only while the room is non-zero, so a screen with no
 * dock over it draws exactly as it did.
 */
export function useDockClearance(ref: RefObject<HTMLElement | null>): void {
  const reach = useMeasured(ref, bottomReach, 0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (reach > 0) {
      el.style.setProperty("--dock-clear-bottom", `${reach}px`);
      el.setAttribute("data-dock-clear", "");
    } else {
      el.style.removeProperty("--dock-clear-bottom");
      el.removeAttribute("data-dock-clear");
    }
  }, [ref, reach]);
}

/** The room an element pinned at the bottom keeps on its right, in px. */
export function useDockOverlap(ref: RefObject<HTMLElement | null>): number {
  return useMeasured(ref, rightReach, 0);
}

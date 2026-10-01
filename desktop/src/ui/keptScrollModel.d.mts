export const KEEP_ATTR: "data-scroll-keep";
export const RESTORE_GRACE_MS: number;
export interface Place {
  top: number;
  left: number;
}
export function restoreStep(
  kept: Place | null | undefined,
  box: { scrollHeight: number; clientHeight: number; scrollWidth: number; clientWidth: number },
): { top: number; left: number; done: boolean };
export function placeOf(scrollTop: number, scrollLeft: number): Place | null;
/** A place read back from a memory that outlives the window, or null for what is no place. */
export function parsePlace(raw: unknown): Place | null;

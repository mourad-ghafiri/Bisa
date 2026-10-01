/**
 * The workstream the Project IDE was last in — what `#/projects` comes back
 * to (D9), and where a door from outside the IDE lands (the Board's chord
 * from another screen). Furniture, in `localStorage`, one id; written by
 * the workbench as its root changes and read by the index and the doors.
 */

import { forgetPref, readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";

const LAST_ROOT_KEY = "bisa.ide.lastRoot";

/** The workbench is on this workstream now. */
export function rememberLastRoot(id: string): void {
  writePref(webStorage(), LAST_ROOT_KEY, id);
}

/** Forget the workstream the IDE was last in — *Forget where I was*. */
export function forgetLastRoot(): void {
  forgetPref(webStorage(), LAST_ROOT_KEY);
}

/** The workstream the IDE was last in, or null on a machine that has not opened one. */
export function lastRoot(): string | null {
  return readPref(webStorage(), LAST_ROOT_KEY, (raw) => raw, null);
}

/**
 * What the machine's clipboard holds that the explorer can paste (ide/03
 * §The explorer): files copied in the file manager, or a picture — what
 * lets the explorer's *Paste* enable and say what it would take. The shell
 * reads the general pasteboard (`pasteboardHolds`); a copy there happens
 * while the app is behind, so the answer is read again when the window
 * comes to the front, when the page becomes visible, and right before a
 * menu opens (`refreshOsClipboard`). Outside the desktop shell the answer
 * is always nothing.
 */

import { useEffect, useSyncExternalStore } from "react";
import { inDesktopShell, pasteboardHolds } from "../api";

/** What the clipboard holds, as the explorer cares: files, a picture. */
export interface OsClipboard {
  files: boolean;
  image: boolean;
}

const NOTHING: OsClipboard = { files: false, image: false };

let held: OsClipboard = NOTHING;
let reading: Promise<void> | null = null;
const listeners = new Set<() => void>();

function set(next: OsClipboard): void {
  if (next.files === held.files && next.image === held.image) return;
  held = next;
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** Read the pasteboard again; one read at a time. */
export function refreshOsClipboard(): Promise<void> {
  if (!inDesktopShell()) {
    set(NOTHING);
    return Promise.resolve();
  }
  if (reading) return reading;
  const read = pasteboardHolds()
    .then((h) => set({ files: h.paths.length > 0, image: h.image }))
    .finally(() => {
      reading = null;
    });
  reading = read;
  return read;
}

/** What the machine's clipboard holds right now. */
export function useOsClipboard(): OsClipboard {
  const value = useSyncExternalStore(subscribe, () => held, () => NOTHING);
  useEffect(() => {
    void refreshOsClipboard();
    const again = () => void refreshOsClipboard();
    window.addEventListener("focus", again);
    document.addEventListener("visibilitychange", again);
    return () => {
      window.removeEventListener("focus", again);
      document.removeEventListener("visibilitychange", again);
    };
  }, []);
  return value;
}

/**
 * Is anyone looking?
 *
 * The app stays open for weeks, so a window minimized or occluded for days
 * should do no read-only polling — no CPU walk, no socket scan, no git-status
 * sweep, no cosmetic clock. There was no such signal before; every poller and
 * ticker ran at full cadence forever. This is the one place that answers
 * "should a background timer run right now", from the standard
 * `document.hidden` / `visibilitychange` (which a Tauri webview fires when the
 * window is minimized or hidden).
 *
 * The rule for callers: **pause read-only polling and cosmetic clocks when
 * hidden, and run once immediately on becoming visible** — so the focused
 * experience is byte-for-byte what it was. Never gate write-side work (autosave)
 * or the event-driven SSE bus on this; those must keep running unseen.
 */

import { useSyncExternalStore } from "react";

function currentlyHidden(): boolean {
  return typeof document !== "undefined" && document.hidden;
}

// The last answer the listeners were told — only to notice a flip. The
// answer itself is always read live: a webview that reported hidden before
// its first paint and never fired `visibilitychange` once latched every
// cosmetic clock off for the life of the window, and the rail's timers then
// moved only when a roster frame happened to arrive.
let hidden = currentlyHidden();
const listeners = new Set<() => void>();

function onChange(): void {
  const next = currentlyHidden();
  if (next === hidden) return;
  hidden = next;
  for (const l of listeners) l();
}

if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", onChange);
  // A window coming to the front, or a page shown again, is a second word
  // on the same fact — read it, in case the first never came.
  window.addEventListener("focus", onChange);
  window.addEventListener("pageshow", onChange);
}

/** Whether the window is hidden (minimized/occluded) right now — read live, never a cached answer. */
export function isHidden(): boolean {
  return currentlyHidden();
}

/**
 * Hear when visibility flips. A module-level scheduler subscribes to
 * re-evaluate whether its timer should run, and to fire once on wake.
 */
export function onVisibilityChange(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** Re-renders a component when visibility flips; `true` when awake. */
export function useVisible(): boolean {
  return useSyncExternalStore(
    (cb) => onVisibilityChange(cb),
    () => !currentlyHidden(),
    () => true,
  );
}

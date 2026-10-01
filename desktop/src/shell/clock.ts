/**
 * One shared, visibility-gated ticker per cadence.
 *
 * Cosmetic clocks — a relative time that ages from "just now" to "2m", a live
 * duration counting up while a session runs — used to each hold their own
 * `setInterval`. A screen with forty relative times ran forty timers; every one
 * kept firing while the window was hidden. This module keeps a single timer per
 * distinct period that every subscriber of that period shares, and pauses it
 * when the window is hidden — resuming with one immediate tick on becoming
 * visible so the text is never stale when looked at again.
 *
 * The rule is the visibility rule (see `visibility.ts`): a cosmetic clock does
 * no work unseen. Never drive write-side work off this.
 */

import { useSyncExternalStore } from "react";
import { isHidden, onVisibilityChange } from "./visibility";

interface Group {
  readonly period: number;
  readonly listeners: Set<() => void>;
  timer: ReturnType<typeof setInterval> | null;
  tick: number;
}

const groups = new Map<number, Group>();

function fire(g: Group): void {
  g.tick += 1;
  for (const l of g.listeners) l();
}

/** Start or stop a group's timer to match visibility and whether anyone reads it. */
function reconcile(g: Group): void {
  const run = g.listeners.size > 0 && !isHidden();
  if (run && !g.timer) {
    g.timer = setInterval(() => fire(g), g.period);
  } else if (!run && g.timer) {
    clearInterval(g.timer);
    g.timer = null;
  }
}

// A window coming back into view catches its clocks up at once, then resumes.
onVisibilityChange(() => {
  for (const g of groups.values()) {
    if (!isHidden() && g.listeners.size > 0) fire(g);
    reconcile(g);
  }
});

function groupFor(period: number): Group {
  let g = groups.get(period);
  if (!g) {
    g = { period, listeners: new Set(), timer: null, tick: 0 };
    groups.set(period, g);
  }
  return g;
}

/**
 * A value that changes every `periodMs` while the window is visible, so a
 * component reading it re-renders on that cadence and no other. All callers of
 * one period share one timer.
 */
export function useClock(periodMs: number): number {
  const g = groupFor(periodMs);
  return useSyncExternalStore(
    (cb) => {
      g.listeners.add(cb);
      reconcile(g);
      return () => {
        g.listeners.delete(cb);
        reconcile(g);
      };
    },
    () => g.tick,
    () => 0,
  );
}

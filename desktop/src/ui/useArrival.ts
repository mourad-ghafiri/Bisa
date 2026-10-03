/**
 * Two memories of a count, for motion that marks a change of state rather
 * than a render.
 *
 * - `useArrivals` counts the times a number *rose* after the first paint —
 *   something new waiting on you, not the list being read again. Keyed on a
 *   wrapper, it replays a one-shot arrival (`motion-pop`) exactly then, and
 *   never when the count falls or a screen mounts with it already standing.
 * - `useCleared` says a list was just emptied while the person watched it:
 *   it held something, in the same scope (the same filter, the same tab),
 *   and now holds nothing. A scope change that empties a list is a different
 *   question, not a cleared one, and resets it.
 *
 * Reduced motion needs nothing here: the classes these drive stop under it
 * (`theme/motion.css`).
 */

import { useEffect, useRef, useState } from "react";

/** How many times `count` has risen since the first paint. */
export function useArrivals(count: number): number {
  const prev = useRef(count);
  const [arrivals, setArrivals] = useState(0);
  useEffect(() => {
    if (count > prev.current) setArrivals((n) => n + 1);
    prev.current = count;
  }, [count]);
  return arrivals;
}

/** Whether `count` just went to zero in `scope` while it was watched; `ready` holds the judgement until the list has loaded. */
export function useCleared(count: number, scope: string, ready = true): boolean {
  const prev = useRef<{ scope: string; count: number } | null>(null);
  const [cleared, setCleared] = useState(false);
  useEffect(() => {
    if (!ready) return;
    const before = prev.current;
    if (before && before.scope === scope && before.count > 0 && count === 0) setCleared(true);
    else if (count > 0 || (before && before.scope !== scope)) setCleared(false);
    prev.current = { scope, count };
  }, [count, scope, ready]);
  return cleared;
}

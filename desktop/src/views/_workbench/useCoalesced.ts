/**
 * One call per burst. A watcher frame arrives per file a save touched — a
 * temp file and a rename, an agent rewriting a page three times in a second
 * — and a reader that answered every frame would read three times for one
 * change. The returned trigger arms a trailing timer once; further triggers
 * inside the window are folded into it; unmounting drops it. The callback
 * is always the latest one handed in, so no caller re-subscribes to keep it
 * fresh.
 */

import { useCallback, useEffect, useRef } from "react";

/** How long a burst of changes is folded into one read. */
const CHANGED_COALESCE_MS = 300;

export function useCoalesced(fn: () => void, ms: number = CHANGED_COALESCE_MS): () => void {
  const latest = useRef(fn);
  latest.current = fn;
  const timer = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = null;
    },
    [],
  );
  return useCallback(() => {
    if (timer.current !== null) return;
    timer.current = window.setTimeout(() => {
      timer.current = null;
      latest.current();
    }, ms);
  }, [ms]);
}

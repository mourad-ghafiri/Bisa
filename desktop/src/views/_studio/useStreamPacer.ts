/**
 * A streamed string revealed at the pacer's rate (`streamPacerModel.mjs`):
 * one animation frame at a time while there is a backlog, none once caught
 * up, and the whole string at once when the turn is done or the person
 * prefers reduced motion. The target grows as frames land; the reveal
 * follows it and is caught up before the next frame.
 */

import { useEffect, useRef, useState } from "react";
import { prefersReducedMotion } from "../../shell/motion";
import { pace, reveal } from "./streamPacerModel.mjs";

export function useStreamPacer(target: string, done: boolean): string {
  const still = done || prefersReducedMotion();
  const [shown, setShown] = useState(() => (still ? target.length : 0));
  const have = useRef(shown);
  useEffect(() => {
    if (still) {
      have.current = target.length;
      setShown(target.length);
      return;
    }
    if (have.current > target.length) {
      have.current = target.length;
      setShown(target.length);
    }
    if (have.current === target.length) return;
    let raf = 0;
    // The frame landed now: the pacer spreads what is left over what is left
    // of the interval since then.
    const landed = performance.now();
    let last = landed;
    const tick = (now: number) => {
      const next = pace({ shown: have.current, target: target.length, dtMs: now - last, sinceMs: now - landed });
      last = now;
      if (next !== have.current) {
        have.current = next;
        setShown(next);
      }
      if (next < target.length) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [target, still]);
  return reveal(target, Math.min(shown, target.length));
}

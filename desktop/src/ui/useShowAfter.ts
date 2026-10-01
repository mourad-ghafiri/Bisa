/**
 * `false` until the beat has passed since the component mounted, then
 * `true` — the one way the kit's indicators (`Pending`, `SkeletonRows`,
 * `Spinner`) wait for the beat `loadingModel.beatMs` names before they show,
 * so a read that answers first never flashes them in a drawn page. Inside a
 * surface that has just opened (`ImmediateIndicators` — a dialog, a popover)
 * the beat is none: the surface is on screen and its status shows with it.
 * One timer, cleared on unmount; a delay of zero is due at once.
 */

import { useContext, useEffect, useState } from "react";
import { IndicatorBeat } from "./indicatorBeat";
import { beatMs, indicatorDue } from "./loadingModel.mjs";

export function useShowAfter(delayMs?: number): boolean {
  const inSurface = useContext(IndicatorBeat);
  const delay = delayMs ?? beatMs(inSurface);
  const [mountedAt] = useState(() => Date.now());
  const [due, setDue] = useState(() => indicatorDue(mountedAt, Date.now(), delay));
  useEffect(() => {
    if (due) return;
    const t = window.setTimeout(() => setDue(true), Math.max(0, delay - (Date.now() - mountedAt)));
    return () => window.clearTimeout(t);
  }, [due, delay, mountedAt]);
  return due;
}

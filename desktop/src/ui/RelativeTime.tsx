// The words are a model (`i18n/format.mjs`), so `node --test` reads them and the
// rail's pulse line uses the same ones; this file draws and ticks.
import { dateTime, durationPrecise, elapsedSince, isoOf, relative } from "../i18n/format.mjs";
import { useClock } from "../shell/clock";
export { relative };

/** The cadence "2m" ages at — one shared, visibility-gated timer. */
const RELATIVE_TICK_MS = 30_000;
/** The cadence a live "open 12s" counter counts up at (matches the old rail clock). */
const LIVE_TICK_MS = 5_000;

/** A moment in full, in the reader's locale — the tooltip behind a relative time. */
export const absolute = (unixSeconds: number): string => dateTime(unixSeconds);

/**
 * Ticks itself so "2m" doesn't sit there reading "just now" all session — and
 * so "in 2m" counts down rather than freezing. Every instance shares one 30 s
 * timer that pauses while the window is hidden (`shell/clock.ts`).
 */
export function RelativeTime({ at, className = "" }: { at: number; className?: string }) {
  useClock(RELATIVE_TICK_MS);
  // A time the node did not give is a dash, not a thrown `RangeError`:
  // `toISOString` is the one date call that throws, and this is drawn on
  // every inbox and pulse row.
  const iso = isoOf(at);
  if (iso === null) return <span className={`tnum text-2xs text-text-dim ${className}`}>—</span>;
  return (
    <time
      dateTime={iso}
      title={absolute(at)}
      className={`tnum text-2xs text-text-dim ${className}`}
    >
      {relative(at)}
    </time>
  );
}

/**
 * A duration that counts up from a fixed start (`since`, unix seconds) — an
 * "open 12s" that advances on its own. A leaf so its ticking never invalidates
 * the model or list that renders it; it shares one 5 s
 * visibility-gated timer with every other live counter (`shell/clock.ts`).
 */
export function LiveDuration({ since, prefix = "", className = "" }: { since: number; prefix?: string; className?: string }) {
  useClock(LIVE_TICK_MS);
  return (
    <span className={`tnum ${className}`}>
      {prefix}
      {durationPrecise(elapsedSince(since, Date.now()))}
    </span>
  );
}

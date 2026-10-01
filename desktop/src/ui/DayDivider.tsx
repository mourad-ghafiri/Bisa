/**
 * A rule with the day written into it, for any list read in time order.
 *
 * Lifted out of the conversation timeline when the Pulse needed the same
 * thing. Two implementations of "Today / Yesterday / a weekday" is two places
 * for the boundary to be computed differently — and the boundary is the whole
 * point, so a feed that disagrees with the conversation it links to about
 * which day a message landed on is worse than a feed with no dividers.
 */

import { Separator } from "./Separator";

// The day's key and its word are `i18n/format.mjs`'s, tested on a fixed clock.
import { dayKey, dayLabel } from "../i18n/format.mjs";
export { dayKey, dayLabel };

/**
 * `sticky` is the caller's decision, not a default, because it is only right
 * in a normally-flowing list. A windowed list positions every row absolutely,
 * where `position: sticky` does nothing at all — so a default of `true` would
 * be a promise the Pulse silently fails to keep.
 */
export function DayDivider({ at, sticky = false }: { at: number; sticky?: boolean }) {
  return (
    // `bg-bg` when sticky: without an opaque band the rules and the label
    // would sit on top of the messages scrolling under them.
    <div
      className={`flex items-center gap-2 px-3 py-1.5 ${
        sticky ? "sticky top-0 z-10 bg-bg" : ""
      }`}
    >
      <Separator className="flex-1" />
      <span className="rounded-full border border-border bg-surface px-2 py-0.5 text-2xs font-medium text-text-dim">
        {dayLabel(at)}
      </span>
      <Separator className="flex-1" />
    </div>
  );
}

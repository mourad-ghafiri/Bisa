/**
 * The shape of content that has not arrived.
 *
 * A skeleton is worth it only where the layout is known before the data is —
 * a list of rows the same height, a card with a fixed header. Where the shape
 * depends on the response, it lies about what is coming and a {@link Spinner}
 * is the honest answer.
 *
 * It is `aria-hidden` and announces nothing. The region it fills should carry
 * its own `aria-busy`, so assistive technology hears "loading" once instead of
 * reading out a paragraph of placeholder blocks.
 *
 * **An indicator earns its place after a beat** (`loadingModel.mjs`): the
 * block, the rows and the *reading …* line all wait `INDICATOR_DELAY_MS`
 * before they show, so a read that answers within it draws nothing but its
 * answer — a placeholder that flashes for thirty milliseconds is worse than
 * none, and a tinted rectangle that does is the worst of them. No piece is
 * exempt: the block a screen sizes itself (`Skeleton`) holds its box unseen
 * for the beat (`placeholderFill`), so nothing jumps and nothing blinks; the
 * immediate rectangle (`Fill`) is this file's own, composed only by the
 * pieces that have already waited. Inside a surface that has just opened the
 * beat is none, as for every indicator (`ImmediateIndicators`).
 */

import { cn } from "./cn";
import { ICON } from "./icons";
import { placeholderFill } from "./loadingModel.mjs";
import { useShowAfter } from "./useShowAfter";
import { t } from "../i18n/l10n.mjs";

/** The rectangle itself, at once — for a piece that has already waited its beat. Never exported. */
function Fill({ className }: { className?: string }) {
  return <span aria-hidden className={cn("block rounded-control", placeholderFill(true), className)} />;
}

/**
 * A placeholder block at the size its call site gives it: the box is held
 * from the first frame, and it is filled — and pulses — only once the beat
 * has passed.
 */
export function Skeleton({ className }: { className?: string }) {
  const due = useShowAfter();
  return <span aria-hidden className={cn("block rounded-control", placeholderFill(due), className)} />;
}

/**
 * A stack of rows at the height the real rows will be, so the list does not
 * jump when they arrive.
 */
export function SkeletonRows({ rows = 5, className }: { rows?: number; className?: string }) {
  const due = useShowAfter();
  if (!due) return null;
  return (
    <div className={cn("flex flex-col gap-1", className)} aria-busy>
      {Array.from({ length: rows }, (_, i) => (
        <Fill key={i} className="h-row w-full" />
      ))}
    </div>
  );
}

/**
 * The place an answer will go, while it is being read: the words *reading
 * {what}…* as a `role="status"` line — they survive reduced motion, where the
 * pulse stops dead — over rows at the height the answer takes. One region,
 * `aria-busy`, so assistive technology hears it once. A panel draws this in
 * the place of the section a read fills, never in the place of the panel —
 * and only once the beat has passed.
 */
export function Pending({ what, rows = 3, className }: { what: string; rows?: number; className?: string }) {
  const due = useShowAfter();
  if (!due) return null;
  return (
    <div className={cn("flex flex-col gap-1.5", className)} aria-busy>
      <div className="flex items-center gap-2 text-2xs text-text-dim" role="status">
        <ICON.working size={12} aria-hidden className="motion-safe:animate-spin" />{t("ui-skeleton-reading", { what })}</div>
      {rows > 0 && (
        <div className="flex flex-col gap-1" aria-hidden>
          {Array.from({ length: rows }, (_, i) => (
            <Fill key={i} className="h-row w-full" />
          ))}
        </div>
      )}
    </div>
  );
}

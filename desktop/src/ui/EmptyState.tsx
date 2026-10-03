/**
 * Empty states are doors, not prose.
 *
 * `action` is required — not optional — so "nothing here" can never ship
 * without someone deciding what the reader should do about it. Pass `null`
 * when there genuinely is no action; that is a decision, and it reads as one
 * in the call site.
 *
 * It lived in `Card.tsx` because it happened to be drawn with a border. It is
 * its own component now for the opposite reason: what it is *for* has nothing
 * to do with cards, and every list in the app ends up needing one.
 */

import type { ReactNode } from "react";
import { cn } from "./cn";
import type { LucideIcon } from "./icons";

export function EmptyState({
  title,
  hint,
  action,
  icon: Icon,
  className,
  cleared = false,
}: {
  title: string;
  hint?: string;
  action: ReactNode;
  /** The concept the list is empty of, from `ui/icons`. */
  icon?: LucideIcon;
  className?: string;
  /**
   * The list was just emptied while the person watched (`useCleared`): the
   * tile arrives once and rests in the success role — the work is done, and
   * the door says so without a word more.
   */
  cleared?: boolean;
}) {
  // Open, not boxed: a dashed frame around "nothing here" reads as a
  // wireframe someone forgot to fill, and a list of them reads as a page
  // under construction. The glyph sits in a soft tile so the door has a
  // shape at a glance; the action gets a beat of air above it.
  return (
    <div
      className={cn(
        "flex flex-col items-center gap-1.5 px-6 py-10 text-center",
        className,
      )}
    >
      {Icon && (
        <span aria-hidden className={cn("anim mb-1.5 flex h-9 w-9 items-center justify-center rounded-control", cleared ? "motion-pop bg-ok-soft text-ok" : "bg-surface-2/70 text-text-dim")}>
          <Icon size={17} />
        </span>
      )}
      <p className="text-sm font-semibold text-text">{title}</p>
      {hint && <p className="max-w-sm text-xs text-text-dim">{hint}</p>}
      {/* The door is never a ghost: a ghost is an action that lives inside a
          row (`Button`), and alone under a sentence it read as more sentence —
          *Show everything*, *Clear filters* — so here it is drawn raised, as
          the default button is. */}
      {action && (
        <div className="mt-2.5 flex flex-wrap items-center justify-center gap-2 [&_[data-variant=ghost]]:border-border [&_[data-variant=ghost]]:bg-surface [&_[data-variant=ghost]]:text-text [&_[data-variant=ghost]]:shadow-sm">
          {action}
        </div>
      )}
    </div>
  );
}

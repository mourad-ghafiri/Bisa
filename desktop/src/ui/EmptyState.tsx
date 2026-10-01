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
}: {
  title: string;
  hint?: string;
  action: ReactNode;
  /** The concept the list is empty of, from `ui/icons`. */
  icon?: LucideIcon;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center gap-2 rounded-card border border-dashed border-border px-6 py-10 text-center",
        className,
      )}
    >
      {Icon && <Icon size={20} aria-hidden className="text-text-dim/60" />}
      <p className="text-xs font-medium text-text">{title}</p>
      {hint && <p className="max-w-sm text-2xs text-text-dim">{hint}</p>}
      {action}
    </div>
  );
}

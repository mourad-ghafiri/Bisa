/**
 * The top of a screen: what you are looking at, and what you can do to it.
 *
 * Every view was building this by hand out of a flex row, and they had
 * drifted — different title sizes, actions sometimes left of the meta and
 * sometimes right. One component so a screen is recognisable as a screen.
 *
 * `actions` is the right-hand slot and should hold at most one primary
 * button. Everything else that acts on the whole screen belongs behind a
 * `Menu` there; a header with five buttons is a header that has not ranked
 * them.
 *
 * `back` is the door to the list this thing came from — a workflow's to
 * Workflows — drawn first as the arrow the Notes and Draw editors wear, its
 * words the tooltip and the accessible name. A screen the sidebar reaches
 * directly has none.
 *
 * **`level` defaults to `h2`, and only the shell's top chrome passes `h1`.**
 * The chrome names the screen on every page, so it owns the page heading; a
 * view that also emitted one would give the document two `h1`s and read the
 * screen's name twice to anyone using a screen reader. A view uses this for
 * the *thing* it is showing — a project, a goal, a team — which is a
 * heading below the screen's, not a second copy of it.
 */

import type { ReactNode } from "react";
import { cn } from "./cn";
import { ICON, type LucideIcon } from "./icons";
import { Tooltip } from "./Tooltip";

export function PageHeader({
  title,
  subtitle,
  icon: Icon,
  back,
  meta,
  actions,
  className,
  level = "h2",
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  /** The concept this screen is about, from `ui/icons`. */
  icon?: LucideIcon;
  /** The door back to the list this thing came from: its words and what it does. */
  back?: { label: string; onClick: () => void };
  /** Chips, counts and timestamps — read, not clicked. */
  meta?: ReactNode;
  actions?: ReactNode;
  className?: string;
  /** Only the shell's chrome passes `h1`; see the note above. */
  level?: "h1" | "h2";
}) {
  const Heading = level;
  return (
    <header className={cn("flex items-start justify-between gap-3 px-4 py-3", className)}>
      <div className="flex min-w-0 items-start gap-2.5">
        {back && (
          <Tooltip label={back.label}>
            <button type="button" aria-label={back.label} onClick={back.onClick} className="anim mt-0.5 shrink-0 rounded-control p-1 text-text-dim hover:bg-surface-2 hover:text-text">
              <ICON.back size={14} aria-hidden />
            </button>
          </Tooltip>
        )}
        {Icon && <Icon size={18} aria-hidden className="mt-0.5 shrink-0 text-text-dim" />}
        <div className="min-w-0">
          {/* The top bar's h1 names the screen and steps back a size, so the
              thing the screen shows — a goal, a team, a run, at text-lg —
              leads; two 18px titles stacked forty pixels apart read as one. */}
          <Heading className={cn("truncate leading-tight font-semibold tracking-tight text-text", level === "h1" ? "text-base" : "text-lg")}>
            {title}
          </Heading>
          {/* No measure cap: a subtitle can be a row that fits itself to the
              header (a channel's topic, then its roster, truncating) and needs
              the header's whole width to decide what it can show. */}
          {subtitle && <p className="mt-1 text-xs text-text-dim">{subtitle}</p>}
          {meta && <div className="mt-1.5 flex flex-wrap items-center gap-1.5">{meta}</div>}
        </div>
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </header>
  );
}

/**
 * The one shell every rail row is drawn through: the depth's margin, the
 * kind's height, rounded corners, the wash, the ink, the cursor ring, the
 * drop-inside ring, and the **attention pill** — a bar at the left edge,
 * accent for the current row or one waiting on you, danger for a failed
 * one — inset from the top and bottom by half the control radius, so the
 * mark and the rounded corners never fight: the corner arc intrudes
 * `r − √(r² − (r − 2)²)` at the pill's edge, and `r / 2` clears that for
 * every radius the families use (`railStyleModel.pillInset`, held by its
 * test against every theme file). Which treatment a
 * row gets is the model's word (`railStyleModel.rowTreatment`); this file
 * maps each word to classes once, in the theme's roles, so every family and
 * accent carries it. `group` stays on the shell so `.row-actions` reveal on
 * hover.
 */

import type { MouseEvent, ReactNode } from "react";
import { CURSOR_RING, cn } from "../../../ui";
import type { TreeRowState } from "../../../ui";
import type { RailTreatment } from "../railStyleModel.mjs";

/** A heading an acceptable row is about to join lights up: the wash here, the ring through `.tree-nest`, both easing. */
const DROP_INTO = "bg-accent-soft/40";
/** The row a drop just put here, for a beat. */
const LANDED = "bg-accent-soft/60";

/** The row's ground: the current root sits on the accent wash; the rest wash on hover. */
const WASH: Record<RailTreatment["wash"], string> = {
  current: "bg-accent-soft",
  rest: "hover:bg-surface-2",
};

/** The bar at the left edge. */
const PILL: Record<Exclude<RailTreatment["pill"], "none">, string> = {
  accent: "bg-accent",
  danger: "bg-danger",
};

/** The ink the row's words are set in; a name's weight is the row's own. */
const INK: Record<RailTreatment["ink"], string> = {
  current: "text-accent-ink",
  strong: "text-text",
  plain: "text-text",
  dim: "text-text-dim",
};

export function RailRow({
  rs,
  treatment,
  tall = false,
  className,
  title,
  onClick,
  onDoubleClick,
  children,
}: {
  rs: TreeRowState;
  /** The model's word for what this row is: its wash, its pill, its ink. */
  treatment: RailTreatment;
  /** The project card's height; every other row is a tree row. */
  tall?: boolean;
  className?: string;
  title?: string;
  onClick?: (e: MouseEvent<HTMLDivElement>) => void;
  onDoubleClick?: (e: MouseEvent<HTMLDivElement>) => void;
  children: ReactNode;
}) {
  const current = treatment.wash === "current";
  return (
    <div
      // A margin, not a padding: the wash begins at the row's depth and ends
      // a step before the edge, so the highlight hugs the row you point at.
      style={{ marginLeft: rs.indent }}
      className={cn(
        "group anim tree-nest relative mr-1 flex items-center rounded-control pl-2 pr-1.5",
        tall ? "h-row gap-2" : "h-row-sm gap-1.5",
        WASH[treatment.wash],
        INK[treatment.ink],
        treatment.muted && "opacity-60",
        rs.dropInside && DROP_INTO,
        rs.landed && LANDED,
        rs.cursor && CURSOR_RING,
        className,
      )}
      data-current={current || undefined}
      data-drop-inside={rs.dropInside || undefined}
      title={title}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      aria-current={current ? "true" : undefined}
    >
      {treatment.pill !== "none" && <span aria-hidden className={cn("pointer-events-none absolute inset-y-[calc(var(--radius-control)/2)] left-0.5 w-0.5 rounded-full", PILL[treatment.pill])} />}
      {children}
    </div>
  );
}

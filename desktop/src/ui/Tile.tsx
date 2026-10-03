/**
 * A tile: one choice among a few, read at a glance — a preview you can
 * recognise, a name, an optional line under it, and the ring every selected
 * state wears. The Appearance panel's theme tiles gave the shape, the pet
 * tiles copied it, the *New drawing* gallery is the third; this is the one
 * place the shape is written.
 *
 * A tile is one control — choosing — so it is a `<button aria-pressed>`; a
 * verb that acts on the thing (*Remove*, *Use*) sits beside the tile, never
 * inside it, as the kit's `Card` rule says.
 */

import type { ReactNode } from "react";
import { cn } from "./cn";
import { ICON } from "./icons";

export function Tile({
  active,
  onSelect,
  name,
  blurb,
  preview,
  previewClass = "flex h-28 items-center justify-center bg-surface-2",
  className,
  disabled,
  title,
  ariaLabel,
  role,
  children,
}: {
  active: boolean;
  onSelect: () => void;
  name: string;
  /** One line under the name; clamped to two. */
  blurb?: string;
  /** What stands in the box. */
  preview: ReactNode;
  /** The box's own classes — its height, its ground, how the preview sits. */
  previewClass?: string;
  /** Extra classes for the tile itself (a row's `flex-1`, say). */
  className?: string;
  disabled?: boolean;
  title?: string;
  ariaLabel?: string;
  /** `radio` inside a `radiogroup`; a plain button otherwise. */
  role?: "radio";
  /** Chips and the like, under the blurb. */
  children?: ReactNode;
}) {
  return (
    <button
      type="button"
      role={role}
      aria-pressed={role ? undefined : active}
      aria-checked={role ? active : undefined}
      aria-label={ariaLabel}
      title={title}
      disabled={disabled}
      onClick={onSelect}
      className={cn("anim group flex min-w-0 flex-col items-stretch gap-1.5 text-left outline-none disabled:opacity-60", className)}
    >
      <span className={cn("anim relative overflow-hidden rounded-card border", previewClass, active ? "border-accent ring-2 ring-accent/40" : "border-border group-hover:border-text-dim/40 group-focus-visible:border-accent")}>
        {preview}
      </span>
      <span className="flex min-w-0 items-center gap-1 text-2xs">
        {active && <ICON.check size={12} aria-hidden className="shrink-0 text-text" />}
        <span className={cn("truncate", active ? "font-medium text-text" : "text-text-dim")}>{name}</span>
      </span>
      {blurb && <span className="line-clamp-2 text-2xs text-text-dim">{blurb}</span>}
      {children}
    </button>
  );
}

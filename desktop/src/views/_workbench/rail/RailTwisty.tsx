/**
 * The disclosure column every rail row has: the chevron on a row that
 * opens, its blank on a leaf — one width (`TWISTY_PX`, `w-4`), so every
 * row's glyph and every level's guide sit in the same column.
 */

import { ICON, cn } from "../../../ui";
import { t } from "../../../i18n/l10n.mjs";

export function RailTwisty({ open, onToggle }: { open: boolean | null; onToggle?: () => void }) {
  if (open === null) return <span aria-hidden className="w-4 shrink-0" />;
  const Glyph = open ? ICON.expanded : ICON.collapsed;
  return (
    <button
      type="button"
      aria-label={open ? t("workbench-rail-twisty-collapse") : t("workbench-rail-twisty-expand")}
      onClick={(e) => {
        e.stopPropagation();
        onToggle?.();
      }}
      // The square washes in `surface-2` on a rest row and in `surface` on the
      // current one, so it never vanishes into the accent wash under it.
      className={cn("anim flex h-4 w-4 shrink-0 items-center justify-center rounded text-text-dim hover:bg-surface-2/70 hover:text-text group-data-[current]:hover:bg-surface/70")}
    >
      <Glyph size={12} aria-hidden />
    </button>
  );
}

/** The column a row's glyph sits in, the twisty's width, centred. */
export function RailGlyph({ children, className }: { children: React.ReactNode; className?: string }) {
  return <span className={cn("flex w-4 shrink-0 items-center justify-center", className)}>{children}</span>;
}

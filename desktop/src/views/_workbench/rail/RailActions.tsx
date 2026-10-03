/**
 * The verbs a rail row reveals on hover, one recipe: a cluster that stops
 * the row's click once, and small square buttons in it — `+`, `⋯`, restart,
 * the stop mark. A menu's trigger is the same square.
 */

import type { ReactNode } from "react";
import { ICON, Menu, Tooltip, cn } from "../../../ui";
import type { MenuItem } from "../../../ui";
import { t } from "../../../i18n/l10n.mjs";

/** The square: it washes in `surface-2` on a rest row and in `surface` on the current one, so it never vanishes into the selected wash. */
const SQUARE = "anim flex h-5 w-5 shrink-0 items-center justify-center rounded text-text-dim hover:bg-surface-2/70 hover:text-text group-data-[current]:hover:bg-surface/70";

export function RailActions({ children, always = false }: { children: ReactNode; always?: boolean }) {
  return (
    <span className={cn("inline-flex shrink-0 items-center gap-0.5", !always && "row-actions")} onClick={(e) => e.stopPropagation()}>
      {children}
    </span>
  );
}

export function RailAction({ label, onClick, danger = false, children }: { label: string; onClick: () => void; danger?: boolean; children: ReactNode }) {
  return (
    <Tooltip label={label}>
      <button type="button" aria-label={label} onClick={onClick} className={cn(SQUARE, danger && "hover:text-danger")}>
        {children}
      </button>
    </Tooltip>
  );
}

/** The row's `⋯`: its context menu, as a button. */
export function RailMenu({ items, label }: { items: MenuItem[]; label: string }) {
  return (
    <Menu
      items={items}
      trigger={
        <span aria-label={label} title={t("workbench-rail-actions-actions")} className={SQUARE}>
          <ICON.more size={14} aria-hidden />
        </span>
      }
    />
  );
}

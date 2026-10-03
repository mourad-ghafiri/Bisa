/**
 * One column of the Board (ide/16): a header with its name, its count (and
 * the overdue among them), the Doing limit's warning, a fold; then the
 * column's cards as one vertical `SortableList` inside a `DropZone`, so a
 * card moves along the column or arrives from another — from a pointer, or
 * from the keyboard with Space. The columns are one sortable **family**: a
 * card hovering another column says so (`onHover`, with the slot it would
 * take) and the Board moves it there at once, so the room it will take is
 * real while it is still in the air.
 */

import type { ReactNode } from "react";
import type { DragData, DropEvent } from "../../ui";
import { DropZone, ICON, SortableList, Tooltip, cn, isDragOf, workstreamCardDrag } from "../../ui";
import type { SortableHandle } from "../../ui";
import { COLUMN_HINT, COLUMN_LABEL, type BoardRow, type Column, headerCounts, wipState } from "./boardModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function BoardColumnView({
  column,
  rows,
  folded,
  onToggleFold,
  today,
  soonDays,
  wipLimit,
  onPlace,
  onHover,
  children,
}: {
  column: Column;
  rows: readonly BoardRow[];
  folded: boolean;
  onToggleFold: () => void;
  today: string;
  soonDays: number;
  wipLimit: number;
  /** A card landed here at an index — from this column or another. */
  onPlace: (id: string, column: Column, index: number) => void;
  /** A card from another column is over this one, at the slot it would take — show it here now. */
  onHover: (id: string, column: Column, index: number) => void;
  children: (row: BoardRow, handle: SortableHandle) => ReactNode;
}) {
  const counts = headerCounts(rows, today, soonDays);
  const wip = wipState(rows.length, column === "doing" ? wipLimit : 0);
  const accepts = (d: DragData) => isDragOf(d, "workstream-card");
  /** Where a foreign card lands: over a card, its slot; anywhere else, the end. */
  const indexOf = (event: DropEvent) => {
    const over = typeof event.over.item === "string" ? rows.findIndex((r) => r.id === event.over.item) : -1;
    return over === -1 ? rows.length : over;
  };

  return (
    <section
      aria-label={COLUMN_LABEL[column]}
      // A quiet well with no edge of its own: the cards in it are the raised, bordered things.
      className={cn("flex min-h-0 flex-col rounded-card bg-surface-2/60", folded ? "w-10 shrink-0" : "w-72 shrink-0")}
    >
      <header className={cn("flex shrink-0 items-center gap-1.5 px-2 py-2", folded && "flex-col")}>
        <button
          type="button"
          onClick={onToggleFold}
          aria-label={folded ? t("board-board-column-view-unfold", { column: COLUMN_LABEL[column] }) : t("board-board-column-view-fold", { column: COLUMN_LABEL[column] })}
          className="anim rounded-control p-0.5 text-text-dim hover:bg-surface-2 hover:text-text"
        >
          {folded ? <ICON.collapsed size={12} aria-hidden /> : <ICON.expanded size={12} aria-hidden />}
        </button>
        <Tooltip label={COLUMN_HINT[column]}>
          <h3 className={cn("min-w-0 truncate text-xs font-semibold text-text", folded && "[writing-mode:vertical-rl]")}>
            {COLUMN_LABEL[column]}
          </h3>
        </Tooltip>
        {!folded && (
          <>
            <span className={cn("tnum text-2xs", wip.over ? "font-medium text-warn" : "text-text-dim")} title={wip.title ?? undefined}>
              {wip.label}
            </span>
            {counts.overdue > 0 && (
              <span className="tnum text-2xs text-danger" title={t("board-board-column-view-overdue-2", { overdue: counts.overdue })}>{t("board-board-column-view-overdue", { overdue: counts.overdue })}</span>
            )}
          </>
        )}
      </header>
      {!folded && (
        <DropZone
          label={t("board-board-column-view-move", { column: COLUMN_LABEL[column] })}
          accepts={accepts}
          onDrop={(data, event) => {
            if (isDragOf(data, "workstream-card")) onPlace(data.id, column, indexOf(event));
          }}
          // The well below the last card: a card over it belongs at the end.
          onHover={(data) => {
            if (isDragOf(data, "workstream-card")) onHover(data.id, column, rows.length);
          }}
          className="min-h-0 flex-1 overflow-y-auto rounded-b-card px-2 pb-2"
          // A column comes back where it was scrolled (`useViewScroll`, on the Board).
          scrollKeep={`board.column:${column}`}
        >
          <SortableList
            items={rows}
            direction="vertical"
            family="board"
            dragData={(row) => workstreamCardDrag(row.id, column, row.title)}
            onReorder={(id, index) => onPlace(id, column, index)}
            onDropForeign={(data, event) => {
              if (isDragOf(data, "workstream-card")) onPlace(data.id, column, indexOf(event));
            }}
            onHover={(data, index) => {
              if (isDragOf(data, "workstream-card")) onHover(data.id, column, index);
            }}
          >
            {children}
          </SortableList>
          {rows.length === 0 && <p className="px-1 py-6 text-center text-2xs text-text-dim">{t("board-board-column-view-nothing-here-drop-card-move-one")}</p>}
        </DropZone>
      )}
    </section>
  );
}

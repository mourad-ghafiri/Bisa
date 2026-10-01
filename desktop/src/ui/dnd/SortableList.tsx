/**
 * A list whose items can be dragged into a new order (ide/03) — the tab
 * strips. Headless: the caller renders each item and is handed the props that
 * make it a drag handle and the transform that slides it aside while another
 * item passes. The list only decides the order.
 *
 * An item dropped on another of the *same* list takes its place
 * (`sortModel.sortableDrop`) and `onReorder` says so. An item from anywhere
 * else dropped on this list — another pane's tab — goes to `onDropForeign`,
 * so a strip can take a tab from its neighbour without the pane behind it
 * having to be the target.
 *
 * Lists of one **family** — a Kanban's columns — name an item the same in
 * every list (`sortModel.sortableId`), so the caller may move the item into
 * the list it hovers *while* it is dragged (`onHover`, with the slot it
 * would take) and the drag keeps tracking it; the neighbours then slide
 * aside through the sortable's own transforms, and the drop lands where the
 * caller already put it.
 */

import { useDndMonitor } from "@dnd-kit/core";
import { SortableContext, horizontalListSortingStrategy, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { useId, useRef, type CSSProperties, type ReactNode } from "react";
import type { DragData } from "./dragData.mjs";
import { useDropHandler, type DropEvent } from "./DragProvider";
import { hoverIndex, sortableDrop, sortableId } from "./sortModel.mjs";

/** What an item's renderer is handed. */
export interface SortableHandle {
  ref: (el: HTMLElement | null) => void;
  /** Spread onto the element that starts the drag. */
  props: Record<string, unknown>;
  style: CSSProperties | undefined;
  dragging: boolean;
}

function SortableItem<T extends { id: string }>({
  listId,
  family,
  item,
  data,
  disabled,
  render,
}: {
  listId: string;
  family: string | null;
  item: T;
  data: DragData | null;
  disabled: boolean;
  render: (item: T, handle: SortableHandle) => ReactNode;
}) {
  const { setNodeRef, listeners, attributes, transform, transition, isDragging } = useSortable({
    id: sortableId(family, listId, item.id),
    data: { ...(data ?? {}), zone: listId, item: item.id, family },
    disabled: disabled || data === null,
  });
  const style: CSSProperties | undefined =
    transform || transition
      ? {
          transform: transform ? `translate3d(${Math.round(transform.x)}px, ${Math.round(transform.y)}px, 0)` : undefined,
          transition: transition ?? undefined,
          opacity: isDragging ? 0.4 : undefined,
        }
      : undefined;
  return <>{render(item, { ref: setNodeRef, props: data ? { ...listeners, ...attributes } : {}, style, dragging: isDragging })}</>;
}

export function SortableList<T extends { id: string }>({
  items,
  direction = "horizontal",
  dragData,
  onReorder,
  onDropForeign,
  onHover,
  family = null,
  disabled = false,
  children,
}: {
  items: readonly T[];
  direction?: "horizontal" | "vertical";
  /** The payload an item carries; null makes that item fixed. */
  dragData: (item: T) => DragData | null;
  /** One of this list's items landed on another: put `id` at `index`. */
  onReorder: (id: string, index: number) => void;
  /** Something from elsewhere landed on this list. */
  onDropForeign?: (data: DragData, event: DropEvent) => void;
  /** An item of the family from another list is over this one, at the slot it would take — move it here now, if the list means to show it. */
  onHover?: (data: DragData, index: number) => void;
  /** Lists an item may move between while dragged share a family; its id is then the same in each. */
  family?: string | null;
  disabled?: boolean;
  children: (item: T, handle: SortableHandle) => ReactNode;
}) {
  const listId = useId();
  const ids = items.map((i) => sortableId(family, listId, i.id));
  const latestHover = useRef(onHover);
  latestHover.current = onHover;
  // A family's item from another list hovering this one: say where it would
  // land. Once the caller has moved it in, it is this list's and the sortable
  // does the rest.
  useDndMonitor({
    onDragOver: (e) => {
      if (!family || !latestHover.current) return;
      const over = e.over?.data.current as { zone?: string; item?: string } | undefined;
      if (!over || over.zone !== listId) return;
      const raw = e.active.data.current as (DragData & { zone?: string; item?: string; family?: string | null }) | undefined;
      if (!raw || raw.family !== family || items.some((i) => i.id === raw.item)) return;
      latestHover.current(raw, hoverIndex(items.map((i) => i.id), over.item ?? null, String(raw.item ?? "")));
    },
  });
  useDropHandler(listId, (e) => {
    const over = typeof e.over.item === "string" ? e.over.item : null;
    const from = (e.data as { zone?: unknown; item?: unknown }).zone === listId ? String((e.data as { item?: unknown }).item ?? "") : null;
    if (from !== null && over !== null) {
      const drop = sortableDrop(
        items.map((i) => i.id),
        from,
        over,
      );
      if (drop) onReorder(from, drop.to);
      return;
    }
    if (from === null) onDropForeign?.(e.data, e);
  });
  return (
    <SortableContext items={ids} strategy={direction === "horizontal" ? horizontalListSortingStrategy : verticalListSortingStrategy} disabled={disabled}>
      {items.map((item) => (
        <SortableItem key={item.id} listId={listId} family={family} item={item} data={dragData(item)} disabled={disabled} render={children} />
      ))}
    </SortableContext>
  );
}

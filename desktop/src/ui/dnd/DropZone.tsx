/**
 * A surface that takes a drop (ide/03): the agent pane taking a file or a
 * hunk, a document pane taking a tab. It says what it accepts, lights up
 * while something acceptable is over it, and is handed the payload when the
 * drag ends there. Nothing about the payload is parsed here; `accepts` is
 * the caller's predicate over the typed union.
 *
 * `useDragSource` is the other half for a thing that is dragged but is not a
 * row of a list — a diff hunk's header, say.
 */

import { useDndMonitor, useDraggable, useDroppable } from "@dnd-kit/core";
import { useId, useRef, type CSSProperties, type ReactNode } from "react";
import { cn } from "../cn";
import type { DragData } from "./dragData.mjs";
import { useActiveDrag, useDropHandler, type DropEvent } from "./DragProvider";

export function DropZone({
  accepts,
  onDrop,
  onHover,
  children,
  className,
  overClassName = "bg-accent-soft/30",
  disabled = false,
  label,
  scrollKeep,
}: {
  /** Whether this payload may land here; nothing lights up for one that may not. */
  accepts: (data: DragData) => boolean;
  onDrop: (data: DragData, event: DropEvent) => void;
  /** An acceptable drag is over the zone itself — its well, not a row inside it. */
  onHover?: (data: DragData) => void;
  children: ReactNode;
  className?: string;
  /** Applied while an acceptable drag is over the zone: a soft fill, eased by `.anim`, never a ring around the whole surface. */
  overClassName?: string;
  disabled?: boolean;
  /** Names the target for assistive technology. */
  label?: string;
  /** The zone scrolls and keeps its place: the name it is kept under (`data-scroll-keep`, `useKeptScroll`). */
  scrollKeep?: string;
}) {
  const zone = useId();
  const { setNodeRef, isOver } = useDroppable({ id: zone, data: { zone }, disabled });
  const active = useActiveDrag();
  const willTake = isOver && active !== null && accepts(active);
  useDropHandler(zone, (e) => {
    if (!disabled && accepts(e.data)) onDrop(e.data, e);
  });
  const latestHover = useRef(onHover);
  latestHover.current = onHover;
  useDndMonitor({
    onDragOver: (e) => {
      if (disabled || !latestHover.current) return;
      const over = e.over?.data.current as { zone?: string } | undefined;
      if (!over || over.zone !== zone) return;
      const raw = e.active.data.current as DragData | undefined;
      if (raw && accepts(raw)) latestHover.current(raw);
    },
  });
  return (
    <div ref={setNodeRef} aria-label={label} data-drop-target={willTake || undefined} data-scroll-keep={scrollKeep} className={cn("anim", className, willTake && overClassName)}>
      {children}
    </div>
  );
}

/**
 * Make one element draggable with a typed payload. Spread `props` onto the
 * element and give it `ref`; `dragging` is true while it is lifted.
 */
export function useDragSource(data: DragData | null): {
  ref: (el: HTMLElement | null) => void;
  props: Record<string, unknown>;
  dragging: boolean;
  style: CSSProperties | undefined;
} {
  const id = useId();
  const { setNodeRef, listeners, attributes, isDragging } = useDraggable({ id, data: data ?? undefined, disabled: data === null });
  return {
    ref: setNodeRef,
    props: data ? { ...listeners, ...attributes, "data-dragging": isDragging || undefined } : {},
    dragging: isDragging,
    // The ghost is the overlay's; the source only fades so the reader can see
    // where it came from.
    style: isDragging ? { opacity: 0.4 } : undefined,
  };
}

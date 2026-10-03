/**
 * A drag handle for a resizable pane.
 *
 * Size is owned by the caller (so it can persist it); this only reports the
 * drag. Double-click resets to the default — a pane you have dragged to a silly
 * size should be one gesture away from sane again.
 *
 * # Why the axis is a table
 *
 * This was horizontal-only, and horizontal in nine separate places: `clientX`,
 * the two `side` values, `col-resize` twice, `aria-orientation`, the two arrow
 * keys, the bar's `w-1`, and the hit area's `-left-1 -right-1`. Adding a bottom
 * panel meant either a second component or nine `if`s interleaving two
 * behaviours through one function. Neither reads. So the nine facts live in one
 * table keyed by axis, and the component below reads from it — which also means
 * the next reader can see, in one place, everything that differs between a
 * vertical and a horizontal handle.
 *
 * `side` is the edge of the resized pane the handle sits on, so a bottom panel
 * uses `side="top"` and gets `size - delta` for free: dragging up makes it
 * taller, which is the mirror of `side="left"`.
 */

import { readPref, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import { useCallback, useEffect, useRef, useState } from "react";
import { clampStored } from "./storedSizeModel.mjs";

type Side = "left" | "right" | "top" | "bottom";

interface Axis {
  /** Which pointer coordinate moves this handle. */
  readonly point: "clientX" | "clientY";
  readonly cursor: string;
  /** A separator's orientation is the line it draws, not the axis it moves on. */
  readonly orientation: "vertical" | "horizontal";
  readonly decrease: string;
  readonly increase: string;
  /** The visible bar, and a hit area wider than it on both sides. */
  readonly bar: string;
  readonly hit: string;
}

const AXES: Record<"x" | "y", Axis> = {
  x: {
    point: "clientX",
    cursor: "col-resize",
    orientation: "vertical",
    decrease: "ArrowLeft",
    increase: "ArrowRight",
    bar: "w-1 cursor-col-resize",
    hit: "absolute inset-y-0 -left-1 -right-1",
  },
  y: {
    point: "clientY",
    cursor: "row-resize",
    orientation: "horizontal",
    decrease: "ArrowUp",
    increase: "ArrowDown",
    bar: "h-1 cursor-row-resize",
    hit: "absolute inset-x-0 -top-1 -bottom-1",
  },
};

/** Which way the pane grows when the pointer moves in the positive direction. */
const GROWS_WITH_POINTER: Record<Side, boolean> = {
  right: true,
  bottom: true,
  left: false,
  top: false,
};

export function ResizeHandle({
  side,
  size,
  min,
  max,
  onSize,
  defaultSize,
  label,
}: {
  /** Which edge of the resized pane this handle sits on. */
  side: Side;
  size: number;
  min: number;
  max: number;
  onSize: (px: number) => void;
  defaultSize: number;
  label: string;
}) {
  const axis = side === "left" || side === "right" ? AXES.x : AXES.y;
  const [dragging, setDragging] = useState(false);
  const start = useRef({ at: 0, size: 0 });

  const onMove = useCallback(
    (e: PointerEvent) => {
      const delta = e[axis.point] - start.current.at;
      const next = GROWS_WITH_POINTER[side]
        ? start.current.size + delta
        : start.current.size - delta;
      onSize(Math.max(min, Math.min(max, Math.round(next))));
    },
    [axis.point, max, min, onSize, side],
  );

  useEffect(() => {
    if (!dragging) return;
    const up = () => setDragging(false);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", up);
    const prev = document.body.style.cursor;
    document.body.style.cursor = axis.cursor;
    document.body.style.userSelect = "none";
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", up);
      document.body.style.cursor = prev;
      document.body.style.userSelect = "";
    };
  }, [axis.cursor, dragging, onMove]);

  return (
    <div
      role="separator"
      aria-orientation={axis.orientation}
      aria-label={label}
      // A separator with a keyboard interaction should report where it is; it
      // used to move under the arrow keys and tell a screen reader nothing.
      aria-valuenow={size}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={(e) => {
        e.preventDefault();
        start.current = { at: e[axis.point], size };
        setDragging(true);
      }}
      onDoubleClick={() => onSize(defaultSize)}
      onKeyDown={(e) => {
        const step = e.shiftKey ? 40 : 8;
        if (e.key === axis.decrease) onSize(Math.max(min, size - step));
        else if (e.key === axis.increase) onSize(Math.min(max, size + step));
        else return;
        e.preventDefault();
      }}
      className={`group relative z-10 shrink-0 ${axis.bar} ${
        dragging ? "bg-text/35" : "bg-transparent hover:bg-text/20"
      } anim`}
    >
      <span className={axis.hit} />
    </div>
  );
}

/**
 * A pane size that survives a reload, read back within the pane's own
 * bounds (`storedSizeModel.clampStored`) — the same `min` and `max` the
 * handle enforces while dragging, so a stored width can never open a pane
 * past the window or a canvas with no width.
 */
export function useStoredSize(key: string, initial: number, bounds?: { min: number; max: number }): [number, (px: number) => void] {
  const [size, setSize] = useState<number>(() => readPref(webStorage(), key, (raw) => clampStored(raw, { initial, ...bounds }), initial));
  const set = useCallback(
    (next: number) => {
      setSize(next);
      writePref(webStorage(), key, String(next));
    },
    [key],
  );
  return [size, set];
}

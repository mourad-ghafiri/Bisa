/**
 * The collapsed Draw overlay: a button you can drag anywhere (19 — Drawings),
 * the notes dock's twin over `ui/Dock` — the raised fill, the full-strength
 * glyph, two edges instead of a shadow, for the reasons `NoteDock.tsx`
 * gives. It starts above the notes' corner so the two never overlap, and
 * wears a working dot while an agent draws.
 */

import { cn } from "../ui/cn";
import { ICON } from "../ui/icons";
import { WorkingDot } from "../ui";
import { useRef } from "react";
import { DOCK_SIZE, dockBox, dockStyle, useDockDrag, useDockViewport } from "../ui/Dock";
import { useBrowserClear } from "../shell/browserClear";
import { useDockFootprint } from "../ui/dockClearance";
import { chordFor } from "../shell/keymapModel.mjs";
import { currentKeymap } from "../shell/shortcuts";
import { useBusyDrawings } from "./drawActivityStore";
import { useDrawingCount } from "./drawingCount";
import { drawingWords } from "./drawRequestModel.mjs";
import { moveDrawDock, toggleDraw, useDrawOverlay } from "./drawStore";
import { t } from "../i18n/l10n.mjs";

export function DrawDock() {
  // The count is the dock's own (`drawingCount.ts`), as the notes dock's is: every drawing there
  // is, kept whether or not the panel is open; `countBadge` says whether to paint it.
  const { dock, countBadge } = useDrawOverlay();
  const count = useDrawingCount();
  const showCount = countBadge;
  const busy = useBusyDrawings();
  const viewport = useDockViewport(DOCK_SIZE);
  const box = dockBox(dock, viewport);
  const drag = useDockDrag(box, viewport, moveDrawDock);
  useDockFootprint("draw", box, DOCK_SIZE);
  // Over a browser tab the layer leaves a round hole for the dock (ide/18), and one for
  // its count badge, as the notes dock's.
  const button = useRef<HTMLButtonElement>(null);
  const badge = useRef<HTMLSpanElement>(null);
  useBrowserClear("draw-dock", button, true);
  useBrowserClear("draw-dock-count", badge, showCount && Boolean(count));
  const chord = chordFor(currentKeymap(), "toggle_draw");

  return (
    <button
      ref={button}
      type="button"
      aria-label={count ? t("draw-dock-drawings-count", { count }) : t("draw-dock-drawings")}
      aria-keyshortcuts={chord ?? undefined}
      title={t("draw-dock-drawings")}
      style={dockStyle(box)}
      onPointerDown={drag.onPointerDown}
      onClick={() => {
        if (drag.wasClick()) toggleDraw();
      }}
      className={cn(
        "anim fixed z-40 flex items-center justify-center rounded-full",
        "border border-border bg-surface-2 text-text",
        "shadow-lg ring-1 ring-inset ring-text/10",
        "hover:border-text-dim hover:ring-text/25 active:scale-95",
        drag.dragging ? "cursor-grabbing" : "cursor-grab",
      )}
    >
      <ICON.draw size={20} strokeWidth={2} aria-hidden />
      {busy.length > 0 && (
        <span className="absolute -top-0.5 -left-0.5">
          <WorkingDot title={drawingWords(busy.length)} />
        </span>
      )}
      {showCount && Boolean(count) && (
        <span ref={badge} aria-hidden className="tnum absolute -top-0.5 -right-0.5 min-w-4 rounded-full border border-border bg-bg px-1 text-3xs leading-tight text-text">
          {count}
        </span>
      )}
    </button>
  );
}

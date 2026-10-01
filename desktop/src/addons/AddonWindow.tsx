/**
 * One addon's window: a sandboxed frame in a box the person can drag — and,
 * as the manifest allows, resize, close, see through (18 — Addons).
 *
 * The frame is `<iframe sandbox="allow-scripts" src=…>` — no
 * `allow-same-origin`, so its origin is opaque; no popups, no forms, no
 * top navigation. Its `src` is the node's token-less files route, never a
 * URL that carries the token. The node's headers fence what the page may
 * load; the shell refuses any navigation off that origin; and the one
 * door out is the bridge (`useAddonBridge`). This component paints the
 * box and moves it; every fact is `addonWindowModel.mjs`'s.
 *
 * A pointer over an iframe is the iframe's — so while any drag or resize
 * runs, the layer lays a **shield** over every frame (`shielded`), and the
 * grip captures the pointer as well.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api } from "../api";
import { t } from "../i18n/l10n.mjs";
import { prefersReducedMotion } from "../shell/motion";
import type { SlotState } from "../shell/layerSlots";
import type { Addon } from "../types";
import { ICON, cn, dockBox, useDockDrag, useDockViewport } from "../ui";
import type { Placement } from "../ui";
import { useAddonBridge } from "./addonBridge";
import type { AddonBridgeHandlers, AddonLoad, AddonSummary } from "./addonBridge";
import { barTitle, entryOf, manifestWindow } from "./addonsModel.mjs";
import { BAR_HEIGHT, clampSize, hiddenByLayer, placementAfterResize, sizeBounds, sizeFrom } from "./addonWindowModel.mjs";
import type { Size } from "./addonWindowModel.mjs";
import { moveAddon, resizeAddon, setAddonHidden, useAddons, windowOf } from "./addonsStore";

export function AddonWindow({
  addon,
  slot,
  slots,
  layerVisible,
  shielded,
  onShield,
  summary,
  load,
  onOpenUrl,
}: {
  addon: Addon;
  /** Its place among every installed addon, for where it first opens — steady while others show or hide. */
  slot: number;
  /** The native layers' boxes: a window under a browser tab hides. */
  slots: readonly SlotState[];
  /** The layer itself shows. */
  layerVisible: boolean;
  /** A drag or resize runs somewhere in the layer. */
  shielded: boolean;
  onShield: (on: boolean) => void;
  summary: AddonSummary;
  load: AddonLoad | null;
  onOpenUrl: (url: string) => Promise<boolean>;
}) {
  const { windowsVersion } = useAddons();
  const w = useMemo(() => manifestWindow(addon.manifest), [addon.manifest]);
  const frame = useRef<HTMLIFrameElement>(null);
  const [title, setTitle] = useState<string | null>(null);

  // A size before a viewport is known, then the stored preference against
  // the real one; the viewport re-measures the size it is told. The
  // preference is re-read when a window moved (`windowsVersion`), never
  // because the list was re-read: the object is not what places it.
  const seed = useDockViewport({ width: w.width, height: w.height });
  const bounds = useMemo(() => sizeBounds(w, seed), [w, seed]);
  // Keyed on the facts above, not the `addon` object: a re-read of the list must not move the window.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const pref = useMemo(() => windowOf(addon, slot, seed), [addon.id, slot, seed.width, seed.height, seed.top, windowsVersion]);
  const [size, setSize] = useState<Size>(pref.size);
  useEffect(() => setSize(pref.size), [pref.size]);
  const [dock, setDock] = useState<Placement>(pref.dock);
  useEffect(() => setDock(pref.dock), [pref.dock]);

  const total = { width: size.width, height: size.height + (w.frame === "none" ? 0 : BAR_HEIGHT) };
  const viewport = useDockViewport(total);
  const box = dockBox(dock, viewport);
  // A drag moves the box on every pointer move and is written once, when it
  // ends — localStorage and every other window are not touched per pixel.
  const drag = useDockDrag(box, viewport, setDock);
  const latest = useRef({ dock, size });
  latest.current = { dock, size };
  const wasDragging = useRef(false);
  useEffect(() => {
    if (wasDragging.current && !drag.dragging) moveAddon(addon.id, latest.current.dock, latest.current.size);
    wasDragging.current = drag.dragging;
  }, [drag.dragging, addon.id]);

  // Resize from the bottom-right corner: the top-left stays where it was.
  const resizing = useRef<{ x: number; y: number; start: Size; dock: Placement } | null>(null);
  const [isResizing, setResizing] = useState(false);
  const onResizeDown = useCallback(
    (e: React.PointerEvent) => {
      if (!w.resizable) return;
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      resizing.current = { x: e.clientX, y: e.clientY, start: size, dock };
      setResizing(true);
      onShield(true);
    },
    [w.resizable, size, dock, onShield],
  );
  const onResizeMove = useCallback(
    (e: React.PointerEvent) => {
      const r = resizing.current;
      if (!r) return;
      const next = sizeFrom(r.start, e.clientX - r.x, e.clientY - r.y, bounds);
      setSize(next);
      setDock(placementAfterResize(r.dock, r.start, next));
    },
    [bounds],
  );
  const onResizeUp = useCallback(() => {
    const r = resizing.current;
    if (!r) return;
    resizing.current = null;
    setResizing(false);
    onShield(false);
    resizeAddon(addon.id, size, dock);
  }, [addon.id, size, dock, onShield]);

  // The shield counts gestures, so it moves on a drag's edges only — never
  // on mount, which would take a count another window's gesture holds — and
  // a window that leaves mid-drag gives its count back.
  const shielding = useRef(false);
  useEffect(() => {
    if (drag.dragging === shielding.current) return;
    shielding.current = drag.dragging;
    onShield(drag.dragging);
  }, [drag.dragging, onShield]);
  useEffect(
    () => () => {
      if (shielding.current) onShield(false);
      // A resize holds a count of its own: a window that leaves mid-resize —
      // its addon switched off, removed — gives it back too, or no frame in
      // the layer would take the pointer again.
      if (resizing.current) onShield(false);
    },
    [onShield],
  );

  const under = hiddenByLayer(box, total, slots);
  const visible = layerVisible && !under;

  const handlers: AddonBridgeHandlers = useMemo(
    () => ({
      onResize: (asked) => {
        const next = clampSize(asked, bounds);
        setSize(next);
        const nextDock = placementAfterResize(dock, size, next);
        setDock(nextDock);
        resizeAddon(addon.id, next, nextDock);
      },
      onClose: () => setAddonHidden(addon.id, true),
      onTitle: (name) => setTitle(name),
      onOpenUrl,
    }),
    [addon.id, bounds, dock, size, onOpenUrl],
  );
  useAddonBridge(frame, addon, { visible, summary, load, handlers });

  const still = prefersReducedMotion();
  // The frame reloads when the record moves — grants are said in the hello.
  const frameKey = `${addon.id}:${addon.installed_at}:${JSON.stringify(addon.granted)}`;
  // A title is the page's that named it: a page loaded afresh has named nothing yet.
  useEffect(() => setTitle(null), [frameKey]);
  const name = barTitle(addon, title);

  return createPortal(
    <div
      role="dialog"
      aria-label={name}
      // content, never translated: the addon's own name in its bar.
      style={{ left: box.left, top: box.top, width: total.width, height: total.height, visibility: visible ? "visible" : "hidden" }}
      className={cn(
        "group fixed z-30 flex flex-col overflow-hidden rounded-control",
        w.transparent ? "bg-transparent" : "border border-border bg-surface shadow-lg",
        !still && !drag.dragging && !isResizing && "anim",
        (drag.dragging || isResizing) && "select-none",
      )}
    >
      {w.frame === "none" ? (
        // No bar: a grip that appears when the pointer is near.
        <button
          type="button"
          aria-label={t("addons-addon-window-move", { name })}
          title={t("addons-addon-window-move", { name })}
          onPointerDown={drag.onPointerDown}
          className="absolute left-1/2 top-0 z-10 -translate-x-1/2 cursor-grab rounded-b-control bg-surface-2/80 px-2 py-px text-text-dim opacity-0 group-hover:opacity-100 focus-visible:opacity-100 active:cursor-grabbing"
        >
          <ICON.grip size={12} aria-hidden />
        </button>
      ) : (
        <div
          style={{ height: BAR_HEIGHT }}
          onPointerDown={drag.onPointerDown}
          className="flex shrink-0 cursor-grab items-center gap-1 border-b border-border bg-surface-2 px-1.5 text-2xs text-text-dim active:cursor-grabbing"
        >
          <ICON.grip size={12} aria-hidden className="shrink-0" />
          {/* content, never translated */}
          <span className="min-w-0 flex-1 truncate font-medium text-text">{name}</span>
          {w.closable && (
            <button
              type="button"
              aria-label={t("addons-addon-window-close", { name })}
              title={t("addons-addon-window-close", { name })}
              onPointerDown={(e) => e.stopPropagation()}
              onClick={() => setAddonHidden(addon.id, true)}
              className="anim flex h-4 w-4 items-center justify-center rounded-control hover:bg-surface-3"
            >
              <ICON.close size={11} aria-hidden />
            </button>
          )}
        </div>
      )}
      <div className="relative min-h-0 flex-1">
        <iframe
          key={frameKey}
          ref={frame}
          title={name}
          sandbox="allow-scripts"
          src={api.addonFileUrl(addon.id, entryOf(addon.manifest))}
          referrerPolicy="no-referrer"
          allow=""
          // @ts-expect-error — the attribute WebKit reads for a see-through frame.
          allowtransparency="true"
          className={cn("block h-full w-full border-0", w.transparent ? "bg-transparent" : "bg-surface")}
          style={w.transparent ? { colorScheme: "normal" } : undefined}
        />
        {/* The shield: while anything in the layer is dragged or resized, no frame takes the pointer. */}
        {shielded && <div aria-hidden className="absolute inset-0" />}
        {w.resizable && (
          <button
            type="button"
            aria-label={t("addons-addon-window-resize", { name })}
            title={t("addons-addon-window-resize", { name })}
            onPointerDown={onResizeDown}
            onPointerMove={onResizeMove}
            onPointerUp={onResizeUp}
            onPointerCancel={onResizeUp}
            className="absolute bottom-0 right-0 h-3 w-3 cursor-nwse-resize touch-none opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
          >
            <span aria-hidden className="absolute bottom-0.5 right-0.5 h-1.5 w-1.5 rounded-sm border-b border-r border-text-dim" />
          </button>
        )}
      </div>
    </div>,
    document.body,
  );
}

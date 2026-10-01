/**
 * The box a native layer is drawn over: empty, its rect published for the
 * host it stands in (`layerSlots.ts`) — the terminal layer
 * (`TerminalPanel.tsx`) or the browser layer (`BrowserPanel.tsx`, ide/18) —
 * and which layer and tab that is. The workbench's centre mounts one while
 * a terminal or browser tab is active; the Details pane's Browser occupant
 * mounts one for the tab it shows.
 */

import { useLayoutEffect, useRef } from "react";
import { LAYOUT_CHANGED, publishLayerSlot, setLayerSlotLayer } from "./layerSlots";
import type { Layer, LayerHost } from "./layerSlots";

export function LayerSlot({ layer, label, host = "center", tabKey = null }: { layer: Layer; label: string; host?: LayerHost; tabKey?: string | null }) {
  const slot = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = slot.current;
    if (!el) return;
    const publish = () => publishLayerSlot(host, el.getBoundingClientRect());
    publish();
    setLayerSlotLayer(host, layer, tabKey);
    const ro = new ResizeObserver(publish);
    ro.observe(el);
    window.addEventListener("resize", publish);
    // A drag of the rail or a pane changes this box's width; the observer
    // sees that. A sidebar or rail toggle moves it without resizing it,
    // which nothing observes — so the togglers announce it
    // (`notifyLayoutChanged`), and a finished CSS transition anywhere on the
    // page re-measures too, on the next frame. No polling.
    let frame: number | null = null;
    const soon = () => {
      if (frame !== null) return;
      frame = window.requestAnimationFrame(() => {
        frame = null;
        publish();
      });
    };
    window.addEventListener(LAYOUT_CHANGED, soon);
    document.addEventListener("transitionend", soon, true);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", publish);
      window.removeEventListener(LAYOUT_CHANGED, soon);
      document.removeEventListener("transitionend", soon, true);
      if (frame !== null) window.cancelAnimationFrame(frame);
      setLayerSlotLayer(host, null);
    };
  }, [layer, host, tabKey]);
  return <div ref={slot} className="h-full w-full" aria-label={label} />;
}

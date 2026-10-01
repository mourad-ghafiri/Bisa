/**
 * Where a native layer is drawn: one slot per host, published by the box
 * the layer stands in.
 *
 * The terminal and browser layers are mounted in `App.tsx`, never inside
 * the routed screen (`TerminalPanel.tsx`, `BrowserPanel.tsx`); a `LayerSlot`
 * publishes the rect of its box here — the workbench's centre body while a
 * terminal or browser tab is active, the Details pane's body while the
 * Browser occupant shows (ide/18) — and the layer draws itself over that
 * rect with `position: fixed`. Two hosts, since the pane and the centre
 * can both be up: the same tab in both draws in the centre, the bigger box.
 * A module store, like `useTerminals`, because the sides are at different
 * depths under different parents. Notifies only on a change (`sameRect`):
 * a notify re-renders every terminal.
 */

import { useSyncExternalStore } from "react";
import { roundRect, sameRect } from "./centerSlotModel.mjs";
import type { Rect } from "./centerSlotModel.mjs";

/** Which native layer a host's box is for, or none. */
export type Layer = "terminal" | "browser";
/** Where a layer may stand: the workbench's centre, or the Details pane. */
export type LayerHost = "center" | "aux";

export interface SlotState {
  readonly rect: Rect | null;
  /** A layer's box is mounted here right now. */
  readonly visible: boolean;
  readonly layer: Layer | null;
  /** The tab the box stands for — a browser tab's key; the terminal layer has its own idea of which. */
  readonly key: string | null;
}

const EMPTY: SlotState = { rect: null, visible: false, layer: null, key: null };
const slots: Record<LayerHost, SlotState> = { center: EMPTY, aux: EMPTY };
const listeners = new Set<() => void>();

function set(host: LayerHost, next: SlotState) {
  const state = slots[host];
  if (next.visible === state.visible && next.layer === state.layer && next.key === state.key && sameRect(next.rect, state.rect)) return;
  slots[host] = next;
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useLayerSlot(host: LayerHost): SlotState {
  return useSyncExternalStore(subscribe, () => slots[host], () => slots[host]);
}

/** The slot as it stands, outside React. */
export function layerSlot(host: LayerHost): SlotState {
  return slots[host];
}

/** Hear every change of every slot; answers the unsubscribe. */
export function subscribeLayerSlots(l: () => void): () => void {
  return subscribe(l);
}

/** A host's box, in viewport pixels; `null` keeps the last one. */
export function publishLayerSlot(host: LayerHost, rect: { left: number; top: number; width: number; height: number } | null): void {
  const rounded = roundRect(rect);
  const state = slots[host];
  set(host, { ...state, rect: rounded ?? state.rect });
}

/** Which layer's box a host holds — whether, which, and for which tab. */
export function setLayerSlotLayer(host: LayerHost, layer: Layer | null, key: string | null = null): void {
  set(host, { rect: slots[host].rect, visible: layer !== null, layer, key: layer === null ? null : key });
}

/**
 * Something moved a host without resizing it — the sidebar or the rail
 * toggled, a panel opened — which a `ResizeObserver` on the slot cannot see.
 * The togglers say so here, and the slot re-measures on the next frame.
 */
export const LAYOUT_CHANGED = "bisa:layout-changed";

export function notifyLayoutChanged(): void {
  window.dispatchEvent(new CustomEvent(LAYOUT_CHANGED));
}

/**
 * The Details pane's slot, as one published fact (`auxSlotModel.mjs`): the
 * pane writes its element through a callback ref when it mounts the slot
 * and `null` when it goes; `AuxPortal` reads it and follows. The shape of
 * `layerSlots.ts` — a module store read through `useSyncExternalStore` —
 * for the same reason: a view and the shell share one fact and neither may
 * go looking in the document for it.
 */

import { useSyncExternalStore } from "react";
import { createSlot } from "./auxSlotModel.mjs";

const slot = createSlot<HTMLElement>();

/** The pane's ref: the element while it is mounted, `null` after. */
export function publishAuxSlot(el: HTMLElement | null): void {
  slot.publish(el);
}

/** The slot a view's `AuxPortal` fills right now, or `null` while the pane is closed or shell-drawn. */
export function useAuxSlot(): HTMLElement | null {
  return useSyncExternalStore(slot.subscribe, slot.current, () => null);
}

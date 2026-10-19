/**
 * The Details pane's Properties panel, as one published fact — the shape of
 * `shell/auxSlot.ts` over `shell/auxSlotModel.createSlot`: the pane's
 * `GoalInspector` writes the panel's element through a callback ref when it
 * mounts the panel and `null` when the panel goes (another tab pressed, the
 * pane closed), and the goal's Workflow tab fills it through
 * `PropertiesPortal`, following a panel that closed and opened again to its
 * new element. So the Workflow tab keeps **ownership** of its editing session
 * — the draft, the selection, the live validation, the run — and only draws
 * its inspector where the pane offers room; the pane owns the frame, the
 * tabs and the width (`bisa.aux.width`). Neither goes looking in the
 * document for the other.
 *
 * The ref must be this module's function itself, never an inline arrow: a
 * callback ref React re-attaches every render would publish `null` and then
 * the element each time, and the portal would remount the inspector per
 * render — focus and drafts lost, which is the fault the panel was rebuilt
 * to end.
 */

import { useSyncExternalStore } from "react";
import { createSlot } from "../../shell/auxSlotModel.mjs";

const slot = createSlot<HTMLElement>();

/** The panel's ref: the element while the Properties panel is mounted, `null` after. */
export function publishPropertiesSlot(el: HTMLElement | null): void {
  slot.publish(el);
}

/** The element the Workflow tab's `PropertiesPortal` fills right now, or `null` while the panel is not showing. */
export function usePropertiesSlot(): HTMLElement | null {
  return useSyncExternalStore(slot.subscribe, slot.current, () => null);
}

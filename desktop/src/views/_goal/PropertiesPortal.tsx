/**
 * Fill the Details pane's Properties panel from the goal's Workflow tab
 * (`propertiesSlot.ts`). Nothing while the panel is not showing — the pane
 * closed, on another tab, or on another occupant — so the tab's inspector
 * costs nothing then; the React tree stays the tab's, so every provider
 * (toasts, tooltips, drag) reaches it as it would in a column of the tab's own.
 */

import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { usePropertiesSlot } from "./propertiesSlot";

export function PropertiesPortal({ children }: { children: ReactNode }) {
  const slot = usePropertiesSlot();
  if (!slot) return null;
  return createPortal(children, slot);
}

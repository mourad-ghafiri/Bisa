/** Types for `contentBoxModel.mjs`. */
import type { Rect } from "./centerSlotModel.mjs";
export type { Rect };
export declare function maximizedStyle(box: Rect | null | undefined): { position: "fixed"; left: number; top: number; width: number; height: number } | null;
export declare function restoreOnEscape(
  appState: { editingTextElement?: unknown; editingLinearElement?: unknown; selectedElementIds?: Record<string, boolean> | null; openMenu?: unknown; openDialog?: unknown; openSidebar?: unknown } | null | undefined,
): boolean;

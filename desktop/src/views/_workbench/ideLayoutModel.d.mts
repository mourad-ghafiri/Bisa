import type { PaneNode } from "../../shell/paneTreeModel.mjs";
import type { WorkbenchTab } from "./workbenchModel.mjs";

export declare const LAYOUT_VERSION: number;
export interface SavedLayout {
  version: number;
  tabs: WorkbenchTab[];
  active: string | null;
  panes: PaneNode;
  pinned: string[];
}
export declare function serializeLayout(
  tabs: readonly WorkbenchTab[],
  activeId: string | null,
  panes?: PaneNode | null,
  pinned?: readonly string[],
  previews?: readonly string[],
  strip?: readonly string[],
): SavedLayout;
export declare function restoreLayout(json: unknown): { tabs: WorkbenchTab[]; active: string | null; panes: PaneNode; pinned: string[]; strip: string[] } | null;
export declare function sameLayout(a: unknown, b: unknown): boolean;

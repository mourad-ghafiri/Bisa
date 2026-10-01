/** Types for `drawModel.mjs`. This file is the only reason TypeScript never has to read it. */

import type { NamedRecord, NoteTab, NoteTarget, OwnerScope, OwnerScopeKind, ScopeNames } from "../notes/notesModel.mjs";
import type { Route } from "../router";

export type DrawTab = NoteTab;
export type { NamedRecord, NoteTarget as DrawTarget, OwnerScope, OwnerScopeKind, ScopeNames };

export declare const DRAW_TABS: readonly DrawTab[];
export declare const DRAW_TAB_LABEL: Record<DrawTab, string>;
export declare function drawTab(raw: string | null | undefined): DrawTab;
export declare function drawTabQuery(tab: DrawTab): string;
export declare function drawTabAdmits(tab: DrawTab, kind: string): boolean;
export declare function drawTabKind(tab: DrawTab): OwnerScopeKind | null;
export declare function drawTargets(
  tab: DrawTab,
  route: Route | null | undefined,
  names: ScopeNames | null | undefined,
  projectOf?: (workstream: string) => string | null | undefined,
): NoteTarget[];
export declare function routeTarget(route: Route | null | undefined, projectOf?: (workstream: string) => string | null | undefined): OwnerScope;
export declare function scopeOfRow(row: { scope: string; scope_id?: string | null } | null | undefined): OwnerScope;
export declare function scopeWords(scope: OwnerScope | null | undefined, names: ScopeNames | null | undefined): string;
export declare function filterDrawings<T extends { title?: string }>(rows: T[], query: string | null | undefined): T[];

/** The two facts a drawing keeps of the canvas's state, as the wire spells them. */
export interface PersistedAppState {
  view_background_color: string;
  grid: boolean;
  [k: string]: unknown;
}
export declare function persistedAppState(appState: { viewBackgroundColor?: unknown; gridModeEnabled?: unknown } | null | undefined): PersistedAppState;
export declare function canvasAppState(saved: { view_background_color?: string; grid?: boolean } | null | undefined): {
  viewBackgroundColor: string;
  gridModeEnabled: boolean;
  gridSize: number;
};
export declare function drawnElements<E extends { isDeleted?: boolean }>(elements: readonly E[]): E[];
export declare function sceneMoved(
  shown: readonly { id: string; version?: number; isDeleted?: boolean }[],
  saved: readonly { id: string; version?: number; isDeleted?: boolean }[],
): boolean;

export declare const SAVE_MIN_MS: number;
export declare const SAVE_MAX_MS: number;
export declare const SAVE_STEP_MS: number;
export declare const SAVE_DEFAULT_MS: number;
export declare function clampSaveDelay(raw: unknown): number;
/** Whether a drawing that could not be read goes without a word: the one kept from the last window, gone since. */
export declare function goneQuietly(id: string | null, restored: string | null, status: number | null | undefined): boolean;
export declare function drawStatusWords(s: { conflict?: string | null; error?: string | null; saving?: boolean; dirty?: boolean }): string;

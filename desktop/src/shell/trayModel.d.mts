/** Types for `trayModel.mjs`. */

import type { InboxRow, SessionRow } from "../types";

export type TrayState = "trouble" | "connecting" | "waiting" | "working" | "paused" | "quiet";
export type CloseVerb = "hide" | "quit";

export interface TrayPrefs {
  closeKeepsRunning: boolean;
  dockIcon: boolean;
}

/** What the shell paints — `tray::TrayReport` on the Rust side, field for field. */
export interface TrayReport {
  state: TrayState;
  needs: number;
  status: string;
  needs_words: string;
}

export interface TrayFacts {
  conn: string;
  everOpen?: boolean;
  paused?: boolean;
  inbox?: readonly InboxRow[] | null;
  sessions?: readonly SessionRow[] | null;
  working?: Readonly<Record<string, readonly string[]>> | null;
}

export declare const TRAY_KEYS: Readonly<{ closeKeepsRunning: "desktop.close_keeps_running"; dockIcon: "desktop.dock_icon" }>;
export declare const TRAY_DEFAULTS: Readonly<TrayPrefs>;
export declare const TRAY_EVENTS: Readonly<{ close: "bisa:close-requested"; quit: "bisa:quit-requested"; go: "tray:go"; dock: "tray:dock" }>;
export declare const TRAY_STATES: readonly TrayState[];
export declare function namesTrayKey(keys: unknown): boolean;
export declare function readTrayPrefs(resolved: readonly { key: string; value: unknown }[] | null | undefined): TrayPrefs;
export declare function closeVerb(prefs: Partial<TrayPrefs> | null | undefined): CloseVerb;
export declare function workingCount(sessions: readonly SessionRow[] | null | undefined, working: Readonly<Record<string, readonly string[]>> | null | undefined): number;
export declare function trayState(facts: { conn: string; everOpen?: boolean; paused?: boolean; needs?: number; working?: number }): TrayState;
export declare function statusWords(state: TrayState | string, working?: number): string;
export declare function trayReport(facts: TrayFacts): TrayReport;

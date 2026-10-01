import type { EnginePayload, ResolvedSetting, SessionRow, SessionState } from "../types";

export type NotifyCategory = "asks" | "failures" | "done" | "workflows" | "addons" | "app";
export interface Notice {
  category: NotifyCategory;
  title: string;
  body: string;
}
export interface NotifyMemory {
  gates: string[];
}
export interface NotifyPrefs {
  enabled: boolean;
  asks: boolean;
  failures: boolean;
  done: boolean;
  workflows: boolean;
  addons: boolean;
}
export declare const MEMORY_CAP: number;
export declare const NOTIFY_KEYS: Readonly<Record<keyof NotifyPrefs, string>>;
export declare const NOTIFY_DEFAULTS: Readonly<NotifyPrefs>;
export declare const CATEGORIES: readonly Exclude<NotifyCategory, "app">[];
export declare function readNotifyPrefs(resolved: readonly ResolvedSetting[]): NotifyPrefs;
export declare function namesNotifyKey(keys: unknown): boolean;
export declare function categoryOfState(word: string): NotifyCategory | null;
export declare function allowed(prefs: NotifyPrefs, category: NotifyCategory): boolean;
export declare function emptyMemory(): NotifyMemory;
export declare function onTransition(prev: SessionState | null, row: SessionRow, memory: NotifyMemory): { notice: Notice | null; memory: NotifyMemory };
export declare function onFrame(payload: EnginePayload, memory: NotifyMemory): { notice: Notice | null; memory: NotifyMemory };

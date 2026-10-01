/** Types for `closeGuardModel.mjs`. */

import type { SettingsTab } from "../views/_settings/settingsLink.mjs";

export interface ConfirmPrefs {
  shell: boolean;
  harness: boolean;
  quit: boolean;
}

/** Where a question's switch lives: the Settings tab, the lead-in, the path in words, and the switch's label — `null` when the question covers both kinds. */
export interface CloseDoor {
  tab: SettingsTab;
  lead: string;
  path: string;
  switch: string | null;
}

export interface CloseQuestion {
  title: string;
  body: string;
  confirmLabel: string;
  door: CloseDoor;
}

export declare const CONFIRM_KEYS: Readonly<{ shell: string; harness: string; quit: string }>;
export declare const CONFIRM_DEFAULTS: Readonly<ConfirmPrefs>;
export declare const CONFIRM_DOORS: Readonly<Record<"shell" | "harness" | "others" | "quit", CloseDoor>>;
export declare function namesConfirmKey(keys: readonly string[] | null | undefined): boolean;
export declare function readConfirmPrefs(resolved: readonly { key: string; value: unknown }[] | null | undefined): ConfirmPrefs;
export declare function confirmsClose(
  session: { liveness?: { status?: string }; harness?: string | null; running?: string | null } | null | undefined,
  prefs: Pick<ConfirmPrefs, "shell" | "harness">,
): boolean;
export declare function closeQuestion(session: { harness?: string | null; running?: string | null } | null | undefined): CloseQuestion;
export declare function othersQuestion(live: number): CloseQuestion;
export declare function quitQuestion(at: { shells: number; harnesses: number; dirty: number }): CloseQuestion;

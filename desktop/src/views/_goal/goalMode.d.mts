/**
 * Types for `goalMode.mjs`. This file is the only reason TypeScript never has
 * to read it.
 */

import type { GoalMode } from "../../types";

export declare const GOAL_MODES: readonly GoalMode[];
export declare const MODE_LABEL: Readonly<Record<GoalMode, string>>;
export declare const MODE_MEANING: Readonly<Record<GoalMode, string>>;
export declare const MODE_ICON: Readonly<Record<GoalMode, "run" | "coreAgent" | "person">>;
export declare const DEFAULT_MODE_KEY: string;
export declare const DEFAULT_MODE: GoalMode;

/** What an auto goal does above a step's ceiling when no guard rule decides: the classifier reads it, or the person is asked. */
export type AutoPermissions = "classify" | "ask";
export declare const AUTO_PERMISSIONS_KEY: string;
export declare const AUTO_PERMISSIONS: readonly AutoPermissions[];
export declare const AUTO_PERMISSIONS_LABEL: Readonly<Record<AutoPermissions, string>>;
export declare const AUTO_PERMISSIONS_MEANING: Readonly<Record<AutoPermissions, string>>;
export declare const DEFAULT_AUTO_PERMISSIONS: AutoPermissions;

export declare function designs(mode: GoalMode | null | undefined): boolean;
export declare function modeOf(goal: { mode?: GoalMode | null } | null | undefined): GoalMode;
export declare function modeSegments(): { id: GoalMode; label: string; icon: "run" | "coreAgent" | "person" }[];
export declare function captureHint(mode: GoalMode): string;
export declare function afterCapture(mode: GoalMode): { tab: "workflow"; edit: "1" } | null;
export declare function captureToast(mode: GoalMode): string;

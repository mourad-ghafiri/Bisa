/** Types for `settingOriginModel.mjs`. */

import type { ResolvedSetting, SettingDef, SettingScope } from "../../types";

/** The scopes Settings writes, narrowest first. */
export declare const SETTINGS_SCOPES: readonly SettingScope[];
export declare function scopeWord(scope: string): string;
/** The badge of a resolved value: its word, its tone, the sentence behind it. */
export declare function originBadge(origin: ResolvedSetting["origin"]): { word: string; tone: "quiet" | "neutral"; hint: string };
export declare function writableScopes(def: Pick<SettingDef, "scopes">): SettingScope[];
export declare function defaultTarget(def: Pick<SettingDef, "scopes">): SettingScope;
export declare function mayReset(origin: ResolvedSetting["origin"], target: SettingScope): boolean;
/** The scope whose value stands over a write at `target`, or null. */
export declare function shadowedBy(origin: ResolvedSetting["origin"], target: SettingScope): string | null;
export declare function shadowWords(origin: ResolvedSetting["origin"], target: SettingScope): string | null;
export declare function setAtWords(scope: SettingScope): string;

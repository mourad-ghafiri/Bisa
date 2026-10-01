/** Types for `browserSettingsModel.mjs`, plain JavaScript so `node --test` reads it. */

export type BrowserPolicy = "assigned" | "everyone" | "nobody";
export type BrowserReach = "anywhere" | "local_only";
export declare const ENABLED_KEY: string;
export declare const HOME_KEY: string;
export declare const REMEMBER_KEY: string;
export declare const AGENTS_KEY: string;
export declare const REACH_KEY: string;
export declare const HEADLESS_KEY: string;
export type HeadlessMode = "unattended" | "always" | "never";
export declare const HEADLESS_MODES: readonly HeadlessMode[];
export declare const DEFAULT_HEADLESS: HeadlessMode;
export declare function headlessSegments(): { id: HeadlessMode; label: string; icon: string }[];
export type ScriptsMode = "allow" | "refuse";
export declare const SCRIPTS_KEY: string;
export declare const SCRIPTS_MODES: readonly ScriptsMode[];
export declare const DEFAULT_SCRIPTS: ScriptsMode;
export declare function scriptsSegments(): { id: ScriptsMode; label: string; icon: string }[];
export declare function scriptsWords(mode: ScriptsMode | string): string;
export declare function headlessWords(mode: string): string;
export declare const POLICIES: readonly BrowserPolicy[];
export declare const DEFAULT_POLICY: BrowserPolicy;
export declare const REACHES: readonly BrowserReach[];
export declare const DEFAULT_REACH: BrowserReach;
export declare function policySegments(): { id: BrowserPolicy; label: string; icon: string }[];
export declare function policyWords(policy: string): string;
export declare function reachWords(reach: string): string;
export declare function statusWords(facts: { available: boolean; enabled: boolean; tabs: number; headless?: number }): { tone: "ok" | "warn" | "quiet"; label: string; sentence: string };

/** Types for `drawSettingsModel.mjs`. */
export declare const ENABLED_KEY: "draw.enabled";
export declare const AGENTS_KEY: "draw.agents";
export declare const SNAPSHOT_WIDTH_KEY: "draw.snapshot.width";
export declare const POLICIES: readonly string[];
export declare const DEFAULT_POLICY: string;
export declare const MIN_SNAPSHOT_WIDTH: number;
export declare const MAX_SNAPSHOT_WIDTH: number;
export declare const DEFAULT_SNAPSHOT_WIDTH: number;
export declare function snapshotWidth(value: unknown): number;
export declare function policyWords(policy: string): string;

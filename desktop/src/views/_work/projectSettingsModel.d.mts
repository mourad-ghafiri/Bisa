import type { ResolvedSetting, SettingOrigin } from "../../types";

export declare const PROJECT_GIT_KEYS: readonly string[];
export declare const PROJECT_WORKSTREAM_KEYS: readonly string[];
export declare const PROJECT_BROWSER_KEYS: readonly string[];
export declare const PROJECT_AGENT_KEYS: readonly string[];
export declare const PROJECT_EDITOR_KEYS: readonly string[];
export declare const PROJECT_KEYS_ELSEWHERE: readonly string[];

export declare function originWords(origin: SettingOrigin): string;

export interface ProjectRow {
  key: string;
  value: unknown;
  origin: string;
  /** The project's own layer holds it — *Inherit* clears it. */
  own: boolean;
}

export declare function projectRow(resolved: ResolvedSetting): ProjectRow;

import type { ScriptEdits } from "./workstreamScripts.mjs";
import type { ConfigWrite } from "./gitConfigModel.mjs";

/** A drafted *Inherit*, compared by identity. */
export declare const INHERIT: Readonly<{ inherit: true }>;
export type Inherit = typeof INHERIT;

export interface Draft {
  settings: Record<string, unknown>;
  /** `null` untouched. */
  publish: string | null;
  /** `undefined` untouched; `null` unpin; a login pins. */
  account: string | null | undefined;
  /** `null` untouched. */
  scripts: ScriptEdits | null;
}

export interface Current {
  settings: Record<string, { value: unknown; own: boolean }>;
  publish: string;
  /** The pinned login, or null. */
  account: string | null;
  scripts: ScriptEdits;
}

export type SettingsWrite =
  | { op: "patch_project"; publish: string }
  | { op: "set_settings"; values: Record<string, unknown> }
  | { op: "unset_setting"; key: string }
  | { op: "git_config"; write: ConfigWrite }
  | { op: "approve_scripts" }
  | { op: "account"; login: string | null };

export declare function emptyDraft(): Draft;
export declare function withSetting(draft: Draft, key: string, value: unknown): Draft;
export declare function withInherit(draft: Draft, key: string): Draft;
export declare function withoutSetting(draft: Draft, key: string): Draft;
export declare function withPublish(draft: Draft, publish: string | null): Draft;
export declare function withAccount(draft: Draft, account: string | null | undefined): Draft;
export declare function withScripts(draft: Draft, scripts: ScriptEdits | null): Draft;
export declare function changedSettings(draft: Draft, current: Current): string[];
export declare function changedScripts(draft: Draft, current: Current): { keys: string[]; texts: boolean };
export declare function changeCount(draft: Draft, current: Current | null, gitWrite: ConfigWrite | null): number;
export declare function problems(draft: Draft, gitProblems?: Record<string, string>): Record<string, string>;
export declare function writes(draft: Draft, current: Current, gitWrite: ConfigWrite | null): SettingsWrite[];
export declare function toolbarWords(count: number, saving: boolean, whole?: boolean): { status: string; save: string };
export declare function savedWords(count: number, approved: boolean): string;
export declare function staleReads(payload: { type?: string; project?: string | null } | null | undefined, pid: string): ("project" | "scripts")[];

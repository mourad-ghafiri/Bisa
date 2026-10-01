import type { GitMergeMode } from "../../types";

export interface ActConsent {
  title: string;
  body: string;
  confirm: string;
  danger: boolean;
  kind: "tree";
}

export declare const MERGE_MODES: readonly GitMergeMode[];
export declare const MERGE_MODE_WORDS: Readonly<Record<GitMergeMode, { label: string; meaning: string }>>;
export declare function mergeDefault(setting: unknown): GitMergeMode;
export declare function mergeTakesMessage(mode: GitMergeMode): boolean;
export declare function mergeMessagePlaceholder(source: string, target: string): string;
export declare function mergeConsent(source: string, target: string, mode: GitMergeMode): ActConsent;
export declare function rebaseConsent(current: string, upstream: string, opts?: { autostash?: boolean; onto?: string | null }): ActConsent;
export declare function pickConsent(count: number, opts?: { recordOrigin?: boolean; noCommit?: boolean }): ActConsent;
export declare function pickOrder(selected: readonly string[], listed: readonly { id: string }[]): string[];

export declare function previewWords(preview: { supported: boolean; clean: boolean; paths: string[] } | null | undefined, opts?: { rebase?: boolean }): { text: string; tone: "dim" | "ok" | "warn" };

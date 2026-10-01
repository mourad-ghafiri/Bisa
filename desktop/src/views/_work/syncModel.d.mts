import type { GitInProgress, GitStatusInfo, PullMode, PullOutcome } from "../../types";

export declare const PULL_MODES: readonly PullMode[];
export declare const PULL_LABEL: Readonly<Record<PullMode, string>>;
export declare const PULL_MEANING: Readonly<Record<PullMode, string>>;
export declare const IN_PROGRESS_LABEL: Readonly<Record<GitInProgress, string>>;

export declare function pullChoice(setting: unknown): PullMode;

export type SyncState =
  | { kind: "not_git" }
  | { kind: "no_remote" }
  | { kind: "detached" }
  | { kind: "no_upstream"; branch: string }
  | { kind: "in_progress"; op: GitInProgress }
  | { kind: "ready"; branch: string; upstream: string; ahead: number; behind: number };

export declare function syncState(status: GitStatusInfo | null | undefined): SyncState;
export declare function syncLine(state: SyncState): string;
export declare function syncControls(state: SyncState): {
  fetch: boolean;
  pull: boolean;
  push: boolean;
  setOrigin: boolean;
};

export type PullResult =
  | { ok: PullOutcome }
  | { err: { status: number; code?: string | null; message: string; detail?: Record<string, unknown> | null } };

export type PullBanner =
  | { kind: "moved"; upstream: string; from: string; to: string; mode: PullMode }
  | { kind: "current"; upstream: string }
  | { kind: "not_fast_forward"; ahead: number; behind: number }
  | { kind: "conflict"; paths: string[]; in_progress: GitInProgress | null; detail: string }
  | { kind: "in_progress"; op: GitInProgress | null }
  | { kind: "error"; detail: string };

export declare function afterPull(result: PullResult): PullBanner;

export interface SyncMenuItem {
  id: string;
  label: string;
  hint: string | null;
  icon: string | null;
  disabled: boolean;
  reason: string | null;
  separatorBefore?: boolean;
  danger?: boolean;
}
export declare function forcePushRule(state: SyncState, defaultBranch: string | null | undefined): { on: boolean; reason: string | null };
export declare function syncMenu(state: SyncState, defaultMode: PullMode, defaultBranch?: string | null): SyncMenuItem[];
/** A pull banner's sentences: the title, and the line under it when there is one. */
export declare function pullBannerWords(banner: PullBanner): { title: string; body: string | null };

import type { FolderRepo } from "../../types";

export type RepoStatus = FolderRepo;

export interface VerbState {
  on: boolean;
  /** The one sentence for a verb that is off; null when it is on. */
  reason: string | null;
}

export interface MenuVerbs {
  push: VerbState;
  fetch: VerbState;
  pull: VerbState;
}

export const ORIGIN: "origin";

export function stripLine(status: RepoStatus, now?: number): string;
export function commitBlockedReason(status: RepoStatus, message: string): string | null;
export function nobodyToCommit(status: RepoStatus): boolean;
export function defaultMessage(now?: number, subject?: string): string;
export function menuVerbs(status: RepoStatus, pulls?: boolean): MenuVerbs;
export function pushOutcomeWords(status: RepoStatus): string;
export function pullOutcomeWords(before: RepoStatus, after: RepoStatus): string;
export function remoteRefusal(value: string): string | null;
export function identityRefusal(name: string, email: string): string | null;

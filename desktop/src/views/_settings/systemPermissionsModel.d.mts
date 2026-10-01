import type { PermissionKind, PermissionStatus } from "../../shell/systemPermissions";

export declare const KINDS: readonly PermissionKind[];
export declare const STATUSES: readonly PermissionStatus[];

export declare function settingKeyOf(kind: PermissionKind): string;
export declare function titleOf(kind: PermissionKind): string;

export interface StatusWords {
  tone: "ok" | "warn" | "neutral" | "quiet";
  label: string;
  sentence: string;
}
export declare function statusWords(kind: PermissionKind, status: PermissionStatus | null, enabled: boolean): StatusWords;
export declare function requestVerb(kind: PermissionKind, status: PermissionStatus | null): string | null;

export interface Recommendation {
  why: string;
  safety: string;
  how: string | null;
}
export declare function recommendation(kind: PermissionKind): Recommendation;
export declare function unavailableWords(reason: "not_desktop" | "not_macos" | string): string;

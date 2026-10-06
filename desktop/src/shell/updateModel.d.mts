export interface ParsedVersion {
  major: number;
  minor: number;
  patch: number;
  pre: string[];
}
export declare function parseVersion(text: unknown): ParsedVersion | null;
export declare function compareVersions(a: string, b: string): -1 | 0 | 1;
export declare function tagFor(version: string): string;
export declare function changelogUrl(repository: string | undefined, tag: string): string | null;
export declare function releasesIndex(repository: string | undefined): string | null;
export declare function notesOf(body: string | null | undefined): string | null;

export interface LatestRelease {
  tag: string;
  version: string;
  name?: string | null;
  published_at?: number | null;
  url: string;
  notes?: string | null;
  prerelease?: boolean;
  assets?: { name: string; url: string; size: number }[];
}
export type UpdateFailure =
  | { kind: "unreachable"; reason: string }
  | { kind: "rate_limited"; retry_in_secs?: number | null }
  | { kind: "unexpected"; status: number };
export type UpdateCheck =
  | { state: "latest"; release: LatestRelease; checked_at: number }
  | { state: "no_release"; checked_at: number }
  | { state: "off" }
  | { state: "failed"; failure: UpdateFailure; checked_at: number };

export type FailureReason = "unreachable" | "rate_limited" | "unexpected" | "node";
export type UpdateState =
  | { kind: "checking"; app: string }
  | { kind: "current"; app: string; version: string; checkedAt: number }
  | {
      kind: "available";
      app: string;
      version: string;
      tag: string;
      name: string | null;
      publishedAt: number | null;
      url: string;
      notes: string | null;
      checkedAt: number;
    }
  | { kind: "ahead"; app: string; latest: string; checkedAt: number }
  | { kind: "none"; app: string; checkedAt: number }
  | { kind: "off"; app: string }
  | { kind: "failed"; app: string; reason: FailureReason; retryInSecs: number | null; checkedAt: number | null };

export declare function updateState(facts: {
  app: string;
  check: UpdateCheck | null | undefined;
  loading: boolean;
  error: string | null | undefined;
}): UpdateState;
export declare function waitWords(secs: number): string;
export declare function versionLine(state: UpdateState): string;
export declare function updateWords(state: UpdateState): { line: string; detail: string | null };
export interface ReleaseLink {
  id: "release" | "changelog";
  label: string;
  url: string;
}
export declare function releaseLinks(release: { url: string; tag: string }, repository: string | undefined): ReleaseLink[];

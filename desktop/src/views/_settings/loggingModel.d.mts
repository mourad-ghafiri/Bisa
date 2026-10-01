/**
 * Types for `loggingModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

export interface LogFileRow {
  name: string;
  family: string;
  period: string;
  size: string;
  age: string;
}

export interface FamilyBlock {
  process: string;
  whose: string;
  dir: string;
  files: LogFileRow[];
}

export interface CrashRow {
  name: string;
  family: string;
  at: string;
  size: string;
  age: string;
}

interface FileLike {
  name: string;
  bytes: number;
  modified_at: number;
}

export declare const LOCAL_ONLY: string;
export declare const FAMILIES: Readonly<Record<"node" | "cli" | "mcp" | "desktop", string>>;
export declare const CRASH_KINDS: Readonly<Record<"panic" | "abrupt_end" | "child_exit", string>>;
export declare function familyOf(name: string): string;
export declare function periodOf(name: string): string;
export declare function stampOf(name: string): string;
export declare function bytesWords(bytes: number): string;
export declare function fileRows(files: readonly FileLike[], now?: number): LogFileRow[];
export declare function familyRows(
  families: readonly { process: string; dir: string; files: readonly FileLike[] }[],
  now?: number,
): FamilyBlock[];
export declare function crashRows(crashes: readonly FileLike[], now?: number): CrashRow[];
export declare function latestCrashWords(
  latest: { process: string; kind: string; at: string; message: string } | null | undefined,
  now?: number,
): string | null;
export declare function totalWords(listing: {
  families: readonly { files: readonly { bytes: number }[] }[];
  crashes: readonly { bytes: number }[];
}): string;

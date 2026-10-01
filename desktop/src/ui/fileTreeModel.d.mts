/**
 * Types for `fileTreeModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 */

import type { FileEntry } from "../types";

export declare const ROOT: "";

export declare function joinPath(base: string, name: string): string;
export declare function parentPath(path: string): string;
export declare function isImmediateChild(parent: string, path: string): boolean;
export declare function isDescendant(parent: string, path: string): boolean;
export declare function formatSize(bytes: number | null | undefined): string;
export declare function sortEntries(entries: readonly FileEntry[]): FileEntry[];

/** One directory's cached listing, and how that listing is doing. */
export interface DirState {
  status: "idle" | "loading" | "ready" | "error";
  entries: FileEntry[];
  truncated: boolean;
  error: string | null;
}

export interface TreeState {
  dirs: Record<string, DirState>;
  /** Path → open. Absent means collapsed; see `collapse` for why it lingers. */
  open: Record<string, true>;
  /** Where the keyboard is. */
  cursor: string | null;
  /** What a verb acts on: one row or many, in the order they were taken. */
  selection: string[];
  /** Where a Shift range starts. */
  anchor: string | null;
  /** The file previewed inline, for a tree that opens files itself. */
  shown: string | null;
  /** The scope's absolute root, once the root listing has answered. */
  root: string | null;
}

export type TreeAction =
  | { type: "reset"; open?: readonly string[] | null }
  | { type: "expand"; path: string }
  | { type: "collapse"; path: string }
  | { type: "toggle"; path: string }
  | { type: "loading"; path: string }
  | { type: "loaded"; path: string; entries: FileEntry[]; truncated: boolean; root?: string }
  | { type: "failed"; path: string; error: string }
  | { type: "collapse_all" }
  | { type: "cursor"; path: string; extend?: boolean }
  | { type: "reveal"; path: string }
  | { type: "select"; path: string }
  | { type: "select_toggle"; path: string }
  | { type: "select_range"; path: string }
  | { type: "select_all" }
  | { type: "select_clear" }
  | { type: "show"; path: string }
  | { type: "hide" }
  | { type: "invalidate" }
  | { type: "refresh_dirs"; paths: readonly string[] };

export declare function initialState(open?: readonly string[] | null): TreeState;
export declare function openFolderPaths(open: Readonly<Record<string, true>>): string[];
export declare function reduce(state: TreeState, action: TreeAction): TreeState;
export declare function pendingLoads(state: TreeState): string[];
export declare function dirsToRefresh(change: { kind: string; path: string; from?: string | null } | null | undefined): string[];

/** A row that is an actual filesystem entry — the only kind the cursor lands on. */
export interface EntryRow {
  kind: "entry";
  /** The path — the row's identity for the generic tree. */
  id: string;
  path: string;
  depth: number;
  entry: FileEntry;
  expanded: boolean;
  /** The entry's name, for type-ahead. */
  label: string;
  /** A directory. */
  expandable: boolean;
}

/** A row standing in for a directory's children rather than for a file. */
export interface NoticeRow {
  kind: "loading" | "error" | "empty" | "truncated";
  id: string;
  /** The directory this is about, not a row of its own. */
  path: string;
  depth: number;
  focusable: false;
  error?: string | null;
}

/** An in-tree draft being named: a new file/folder, or a picture pasted from the clipboard. */
export interface DraftRow {
  kind: "draft";
  id: string;
  path: string;
  depth: number;
  focusable: false;
  /** The directory it will be created in (the root is ""). */
  dir: string;
  entryKind: "file" | "dir" | "image";
}

export type TreeRow = EntryRow | NoticeRow | DraftRow;

export declare function visibleRows(state: TreeState): (EntryRow | NoticeRow)[];
export declare function bodyRows(state: TreeState): (EntryRow | NoticeRow)[];
export declare function withDraft(
  rows: readonly TreeRow[],
  draft: { dir: string; kind: "file" | "dir" | "image" } | null | undefined,
): TreeRow[];

export declare function activate(rows: readonly TreeRow[], cursor: string | null): { kind: "toggle" | "open"; path: string } | null;

/** A file or folder a verb acts on. */
export interface Target {
  path: string;
  dir: boolean;
}
export declare function rangeBetween(rows: readonly TreeRow[], a: string | null, b: string): string[];
export declare function targetsOf(rows: readonly TreeRow[], selection: readonly string[], at: string | null): Target[];
export declare function loadKey(pending: string[]): string;

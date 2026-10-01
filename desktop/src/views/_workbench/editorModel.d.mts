/** Types for `editorModel.mjs`. */

export declare const AUTOSAVE_FLOOR_MS: number;
/** The size a file is drawn plain above — the editable size nobody set. */
export declare const EDITABLE_BYTES: number;
export declare const AUTOSAVE_CEILING_MS: number;
export type BufferStatus =
  | "loading"
  | "clean"
  | "dirty"
  | "saving"
  | "conflict"
  | "read_only"
  | "binary"
  | "error";

export interface Conflict {
  theirs: string;
  theirsHash: string | null;
}

export interface Buffer {
  path: string;
  status: BufferStatus;
  text: string;
  savedText: string;
  savedHash: string | null;
  dirtySince: number | null;
  readOnly: boolean;
  /** Why it cannot be typed into — the node's word: over the editable size, or cut where the read stops. */
  readOnlyWhy: "size" | "truncated" | null;
  plain: boolean;
  binary: boolean;
  size: number;
  conflict: Conflict | null;
  error: string | null;
  /** What the read was refused for, while the document shows a refusal. */
  refusal: LoadFailure | null;
}

/** What a read that threw means; a file over the size the editor opens carries the size and the limit the node said. */
export type LoadFailure =
  | { kind: "too_large"; message: string; size: number | null; limit: number | null }
  | { kind: "missing"; message: string }
  | { kind: "failed"; message: string };

export interface LoadedFile {
  text?: string | null;
  hash?: string | null;
  editable: boolean;
  truncated: boolean;
  binary: boolean;
  size: number;
}

/** Where a buffer comes from: a file under the root, a loose file on this machine by absolute path, or an untitled document not yet saved anywhere. */
export type DocSource = { kind: "file"; path: string } | { kind: "loose"; path: string } | { kind: "untitled"; seq: number };

export declare function isUntitled(source: DocSource | null | undefined): source is { kind: "untitled"; seq: number };
export declare function isLoose(source: DocSource | null | undefined): source is { kind: "loose"; path: string };
export declare function sourceName(source: DocSource | null | undefined): string;
export declare function basenameOf(path: string | null | undefined): string;
export declare function untitledBuffer(source: DocSource): Buffer;
/** A sentence refusing the path a new document would be saved under, or null. */
export declare function savePathProblem(value: string | null | undefined): string | null;
export declare function emptyBuffer(path: string): Buffer;
export declare function loaded(buffer: Buffer, file: LoadedFile): Buffer;
export declare function loadFailure(error: unknown): LoadFailure;
export declare function loadFailed(buffer: Buffer, failure: LoadFailure): Buffer;
/** The line under a refused read, or null when the node's sentence says it all. */
export declare function refusalWords(refusal: LoadFailure | null | undefined): string | null;
/** Why a document cannot be typed into, or is drawn plain, as the bar says it; null when there is nothing to say. */
export declare function sizeNote(buffer: Pick<Buffer, "readOnly" | "readOnlyWhy" | "plain">): string | null;
/** A save the node refused for its size, with the limit the node said; null when the answer carried none. */
export declare function overBoundWords(size: number | null, limit: number | null): string | null;
export declare function edited(buffer: Buffer, text: string, now: number): Buffer;
export declare function isDirty(buffer: Buffer): boolean;
export declare function isUnsaved(buffer: Buffer | null | undefined): boolean;
export declare function unsavedKeys(buffers: ReadonlyMap<string, Buffer>): string[];
export declare function remounted(kept: Buffer | null | undefined, path: string): Buffer;
export declare function saveStarted(buffer: Buffer): Buffer;
/** The formatter answered during a save: a buffer nobody touched since takes its text; one typed in since is left as typed. */
export declare function reformatted(buffer: Buffer, typed: string, text: string): Buffer;
export declare function saved(buffer: Buffer, text: string, hash: string): Buffer;
export declare function conflicted(buffer: Buffer, currentText: string, currentHash: string | null): Buffer;
export declare function saveFailed(buffer: Buffer, error: unknown): Buffer;
export declare function changedOnDisk(buffer: Buffer, file: LoadedFile): Buffer;
export declare function keepMine(buffer: Buffer): Buffer;
export declare function takeTheirs(buffer: Buffer): Buffer;
export declare function clampAutosaveDelay(value: unknown, fallback?: number): number;
export declare function autosaveDue(buffer: Buffer, mode: unknown, delayMs: unknown, now: number): boolean;
export declare function breadcrumbsOf(path: string): { label: string; path: string; dir: boolean }[];
export declare function autosaveFrom(rows: readonly { key: string; value: unknown }[] | null | undefined): { mode: string; delay: number };
export declare const DEFAULT_TAB_SIZE: number;
export declare function formatOnSave(resolved: readonly { key: string; value: unknown }[] | null | undefined, lspLanguage: string | null | undefined): { tabSize: number; insertSpaces: boolean } | null;
export declare function saveFailure(error: unknown): { kind: "conflict"; text: string; hash: string | null } | { kind: "failed"; message: string };

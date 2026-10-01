export const ROOT_FOLDER: "";
export const ROOT_ID: string;
export const TYPED_ID: string;
export const FILTER_LIMIT: number;
export const MEMORY_KEY: string;
export const MEMORY_CAP: number;

/** One row of the picker — the tree's or the filter's. A `TreeRowLike`. */
export interface FolderRow {
  kind: "root" | "folder" | "match" | "typed" | "loading" | "error";
  id: string;
  /** The folder the row serves; null for a notice. */
  folder: string | null;
  depth: number;
  label: string;
  expandable: boolean;
  expanded: boolean;
  ignored: boolean;
  focusable?: boolean;
  error?: string | null;
}

interface ServedLike {
  owner: { kind: string; folder?: string };
  port: number;
}

export function idOfFolder(folder: string): string;
export function folderProblem(folder: string): string | null;
export function normalizeFolder(typed: string): string;
export function foldersOf(paths: readonly string[]): string[];
export function indexFolders(paths: readonly string[]): Set<string>;
export function serverFor<S extends ServedLike>(servers: readonly S[], folder: string): S | null;
export function folderRows(state: object, rootLabel: string): FolderRow[];
export function listedFolders(state: object): string[];
export function filterRows(query: string, folders: readonly string[], rootLabel: string): FolderRow[];
export function filterProblem(query: string): string;
export function stepCursor(rows: readonly { id: string; focusable?: boolean }[], cursor: string | null, step: 1 | -1): string | null;
export function ancestorsOf(folder: string): string[];
export function submitWords(server: { port: number } | null): string;
export function choiceWords(folder: string, server: { port: number } | null): string;
export function parseRemembered(raw: string | null): Record<string, string>;
export function rememberedFolder(byCheckout: Record<string, string> | null | undefined, wid: string): string;
export function rememberFolder(byCheckout: Record<string, string> | null | undefined, wid: string, folder: string, cap?: number): Record<string, string>;

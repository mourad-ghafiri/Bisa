/**
 * How the keymap reaches a file tree (ide/03, ide/15). The `files` commands —
 * rename, open, delete, cut, copy, paste, duplicate, new file, new folder —
 * are chords a person can rebind, so the shortcut handler cannot know what a
 * tree does about them; it asks the *focused* tree through this store, and
 * the tree owns the verb. A reveal ("show this path in Files") is addressed
 * to a root instead, and waits for that root's tree to mount if it has not.
 *
 * The Files search box is here for the same reason: `search_files` opens it
 * from anywhere in the root, and the panel that draws it may not be mounted
 * when the chord lands.
 */

import { useSyncExternalStore } from "react";

const EXPLORER_COMMANDS = Object.freeze([
  "rename_entry",
  "open_entry",
  "delete_entry",
  "copy_entry",
  "cut_entry",
  "paste_entry",
  "duplicate_entry",
  "new_file",
  "new_folder",
  "select_all_entries",
] as const);

export type ExplorerCommand = (typeof EXPLORER_COMMANDS)[number];

export function isExplorerCommand(id: string): id is ExplorerCommand {
  return (EXPLORER_COMMANDS as readonly string[]).includes(id);
}

export interface ExplorerHandle {
  /** A keymap command landed while this tree had focus. */
  command: (cmd: ExplorerCommand) => void;
  /** Open every folder above `path`, put the cursor on it and scroll it into view. */
  reveal: (path: string) => void;
}

const trees = new Map<string, ExplorerHandle>();
let focused: string | null = null;
const pendingReveal = new Map<string, string>();

/** A tree mounts under its root key (`scope:id`). A reveal that arrived first is delivered now. */
export function registerExplorer(key: string, handle: ExplorerHandle): () => void {
  trees.set(key, handle);
  const waiting = pendingReveal.get(key);
  // The person moved on: a reveal asked for another root, whose tree never
  // mounted, is not delivered to it months later.
  pendingReveal.clear();
  if (waiting !== undefined) handle.reveal(waiting);
  return () => {
    if (trees.get(key) === handle) trees.delete(key);
    if (focused === key) focused = null;
  };
}

/** The tree that has keyboard focus, or none. */
export function explorerFocused(key: string | null): void {
  focused = key;
}

/** Deliver a `files` command to the focused tree. False when no tree has focus. */
export function sendExplorerCommand(cmd: ExplorerCommand): boolean {
  const handle = focused ? trees.get(focused) : undefined;
  if (!handle) return false;
  handle.command(cmd);
  return true;
}

/** Show `path` in the tree for `key`, now or when it mounts. */
export function requestReveal(key: string, path: string): void {
  const handle = trees.get(key);
  if (handle) handle.reveal(path);
  else pendingReveal.set(key, path);
}

// ---------------------------------------------------------------------------
// The Files search box, per root.
// ---------------------------------------------------------------------------

export interface ExplorerSearchState {
  readonly open: boolean;
  /** Bumped on every open request, so an already-open box refocuses. */
  readonly nonce: number;
}

const CLOSED: ExplorerSearchState = { open: false, nonce: 0 };
let search: Readonly<Record<string, ExplorerSearchState>> = {};
const searchListeners = new Set<() => void>();

function subscribeSearch(l: () => void): () => void {
  searchListeners.add(l);
  return () => {
    searchListeners.delete(l);
  };
}

function setSearch(key: string, next: ExplorerSearchState): void {
  search = { ...search, [key]: next };
  for (const l of searchListeners) l();
}

export function useExplorerSearch(key: string): ExplorerSearchState {
  return useSyncExternalStore(
    subscribeSearch,
    () => search[key] ?? CLOSED,
    () => CLOSED,
  );
}

export function openExplorerSearch(key: string): void {
  const prev = search[key] ?? CLOSED;
  setSearch(key, { open: true, nonce: prev.nonce + 1 });
}

export function closeExplorerSearch(key: string): void {
  const prev = search[key] ?? CLOSED;
  if (prev.open) setSearch(key, { open: false, nonce: prev.nonce });
}

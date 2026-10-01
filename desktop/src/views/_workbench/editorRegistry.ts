/**
 * What the workbench needs of its open editors without owning them: how to
 * ask each to save (for *Save* on a close), reveal a line, read a selection.
 * Mounted editors register a handle; unmounting forgets the handle — and
 * nothing else. **Which documents hold unsaved work is not a fact about what
 * is mounted**: a pane draws one tab at a time, so it is read from the
 * buffers themselves (`docBuffersStore.ts`), and a document off screen counts
 * like any other. Saving one brings it on screen first ({@link saveDocument}):
 * the editor's own save is the one place that names an untitled document,
 * merges a conflict and says why a save failed.
 *
 * A document that is not a workbench tab — the workflow designer's draft —
 * registers as a *dirty source*: something the quit question counts and the
 * close flow saves, without a tab, a dot or a line to reveal. Which editor is
 * active, which line waits and what the quit question counts are the facts of
 * `editorRegistryModel.mjs`; the handles and the listeners live here.
 */

import { errorFields, log } from "../../log";
import { saveEvery } from "../../shell/closeFlowModel.mjs";
import { unsavedNow, useUnsavedKeys } from "./docBuffersStore";
import { INITIAL, activated, dirtyCount as dirtyCountOf, lineRequested, lineTaken, released } from "./editorRegistryModel.mjs";
import type { RegistryState } from "./editorRegistryModel.mjs";

interface EditorHandle {
  /** Save now. Resolves `true` when the buffer is clean afterwards. */
  save: () => Promise<boolean>;
  /** Jump to a line — quick open's `:42`. `false` when the editor is not up yet. Absent for a rendered or binary document. */
  revealLine?: (line: number) => boolean;
  /** The document's path in its root, for chips that name it. */
  path?: string;
  /** The current selection, for *attach selection* (ide/09). */
  selection?: () => { start: number; end: number; text: string } | null;
  /** Run one of the editor's own actions — the find and replace widgets under the app's chords. */
  trigger?: (action: string) => void;
}

/**
 * Which editor is active and which line waits — the rules are the model's
 * (`editorRegistryModel.mjs`); this is the one copy of its state.
 */
let registry: RegistryState = INITIAL;

/**
 * The editor the person is looking at — the one that last took focus — for
 * a `:line` jump, *attach selection*, ⌘S. Several editors are mounted at
 * once in a split, so a mount claims the slot only until another is focused,
 * and an unmount releases only its own claim.
 */
export function setActiveEditor(key: string): void {
  registry = activated(registry, key);
}
export function releaseActiveEditor(key: string): void {
  registry = released(registry, key);
}
export function activeEditor(): EditorHandle | null {
  return registry.active ? (editors.get(registry.active)?.handle ?? null) : null;
}

interface Entry {
  handle: EditorHandle;
}

let editors: ReadonlyMap<string, Entry> = new Map();
const listeners = new Set<() => void>();

function set(next: Map<string, Entry>) {
  editors = next;
  for (const l of listeners) l();
}

export function registerEditor(key: string, handle: EditorHandle): () => void {
  const next = new Map(editors);
  next.set(key, { handle });
  set(next);
  return () => {
    // Only its own registration: a document that moved to another pane is
    // registered by the editor that mounted there, and the one leaving must
    // not take that handle with it — as a dirty source releases only itself.
    if (editors.get(key)?.handle !== handle) return;
    const after = new Map(editors);
    after.delete(key);
    set(after);
  };
}

/**
 * A line to show once the document can — a search hit, a definition, a `:42`
 * — asked for before its editor is mounted. Shown now when it can be; held
 * until {@link takeLine} otherwise. Asked again for another line, the newer
 * one wins; asked for another document, the older request is dropped: the
 * person has moved on.
 */
export function requestLine(key: string, line: number): void {
  const shownNow = editors.get(key)?.handle.revealLine?.(line) === true;
  registry = lineRequested(registry, key, line, shownNow);
}
/** The line waiting for this document, once, or null. */
export function takeLine(key: string): number | null {
  const taken = lineTaken(registry, key);
  registry = taken.state;
  return taken.line;
}

/** Something outside the workbench that can hold unsaved work. */
export interface DirtySource {
  dirty: () => boolean;
  /** Save now. Resolves `true` when nothing is left unsaved. */
  save: () => Promise<boolean>;
}

let sources: ReadonlyMap<string, DirtySource> = new Map();

export function registerDirtySource(key: string, source: DirtySource): () => void {
  const next = new Map(sources);
  next.set(key, source);
  sources = next;
  return () => {
    const after = new Map(sources);
    if (after.get(key) === source) {
      after.delete(key);
      sources = after;
    }
  };
}

function dirtySources(): DirtySource[] {
  return [...sources.values()].filter((s) => s.dirty());
}

/** The set of keys holding unsaved work, as `${rootKey}|${tabId}` — stable while its members are. */
export function useDirtyEditors(): ReadonlySet<string> {
  return useUnsavedKeys();
}

export function anyDirty(): boolean {
  return dirtyCount() > 0;
}

/** How many documents hold unsaved work — what the quit question names. */
export function dirtyCount(): number {
  return dirtyCountOf(unsavedNow(), [...sources.values()]);
}

/**
 * How a document that is not on screen is brought there — the workbench's
 * to say, for the root it shows: it answers whether the key is one of its
 * tabs. A document of a root that is not on screen has nobody to show it.
 */
type Revealer = (key: string) => boolean;
let revealer: Revealer | null = null;
export function setRevealer(next: Revealer): () => void {
  revealer = next;
  return () => {
    if (revealer === next) revealer = null;
  };
}

/** How long a document brought on screen is given to mount its editor. */
const MOUNT_GRACE_MS = 3000;

/** The handle of `key` once its editor has registered; null when it never does. */
function whenRegistered(key: string): Promise<EditorHandle | null> {
  const now = editors.get(key)?.handle;
  if (now) return Promise.resolve(now);
  return new Promise((resolve) => {
    const done = (handle: EditorHandle | null) => {
      listeners.delete(check);
      clearTimeout(timer);
      resolve(handle);
    };
    const check = () => {
      const handle = editors.get(key)?.handle;
      if (handle) done(handle);
    };
    const timer = setTimeout(() => done(null), MOUNT_GRACE_MS);
    listeners.add(check);
  });
}

/**
 * Save one document, on screen or not. Off screen it is shown first, then
 * saved by its own editor. Resolves `true` when nothing of it is left
 * unsaved; `false` leaves it on screen saying why, or names nobody to show it.
 */
export async function saveDocument(key: string): Promise<boolean> {
  if (!editors.has(key) && !revealer?.(key)) return false;
  const handle = await whenRegistered(key);
  if (!handle) return false;
  return handle.save().catch((e: unknown) => {
    // The editor's save says its own failures; one that throws said nothing.
    log.warn("editor", "a document's save threw", { document: key, ...errorFields(e) });
    return false;
  });
}

/**
 * Save every document holding unsaved work and every dirty source. Resolves
 * `true` when all of them are clean afterwards — what a window close waits
 * for before it goes through. The documents go one at a time: a pane shows
 * one tab, and one that is off screen is shown to be saved.
 */
export async function flushAll(): Promise<boolean> {
  const documents = await saveEvery(unsavedNow(), saveDocument);
  const sources = await saveEvery(
    dirtySources(),
    (s) => s.save(),
    (_, e) => log.warn("editor", "a draft's save threw", errorFields(e)),
  );
  return documents && sources;
}

export function editorKey(rootKey: string, tabId: string): string {
  return `${rootKey}|${tabId}`;
}

/**
 * What the Git tab is doing and has typed, **per checkout**, kept where a tab
 * switch cannot reach it (ide/04 §What survives a switch). Every right-panel
 * occupant and every Git view unmounts when another is shown, and a mutation
 * that lands its answer in a component's `setState` lands it on nothing. So
 * the operations (`gitOps.ts`) write here, and the panels read here through
 * `useSyncExternalStore` — a result lands whether or not anyone is looking,
 * and the next mount shows it. Keyed by the root scope (`workstream:<id>`),
 * so what was typed for one checkout never turns up on another.
 *
 * Two tiers (`gitPanelModel.mjs`): the **draft** — the commit message —
 * persisted as `bisa:git:<scope>` and removed when it empties, like the
 * composer's `bisa:draft:<scope>`; the **session** — busy, the note, the
 * banners, the last failure, the selection, the folds of the moment — in
 * memory only, capped at `MAX_SESSIONS` scopes. `useSessionDraft` is the
 * same memory tier for one component's own draft (a branch name, a merged
 * file, a hunk's picks), keyed by whatever makes it *this* draft.
 *
 * **How the panel stood outlives the window**: the view part of a session —
 * the selection, the folds, the history's filter (`gitViewOf`) — is kept
 * under `git:<scope>` in the view memory, quietly, and laid onto a clean
 * session the first time a scope is met. Nothing an operation left comes
 * back with it.
 *
 * `snapshot()` returns stored objects by identity (`workbenchStore.ts`'s rule):
 * React 19 refuses a getSnapshot that builds a fresh object.
 */

import { useSyncExternalStore } from "react";
import { Lru } from "../../shell/lru.mjs";
import { readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { viewState } from "../../shell/viewMemoryStore";
import { sameKept } from "../../shell/viewValuesModel.mjs";
import type { GitFileRow } from "../../types";
import { gitPlace } from "../_workbench/idePlacesModel.mjs";
import { keepSelection } from "./gitFiles.mjs";
import { landedGitFiles } from "./gitFilesStore";
import { EMPTY_DRAFT, EMPTY_SESSION, MAX_SESSION_DRAFTS, draftStorageKey, gitViewOf, remember, sameGitView, withGitView } from "./gitPanelModel.mjs";
import type { GitDraft, GitSession } from "./gitPanelModel.mjs";

/** The one name a panel's view is kept under, in its place. */
const VIEW = "view";
/** What a fresh panel's view is: worth no memory. */
const FRESH_VIEW = gitViewOf(EMPTY_SESSION);

let drafts: Readonly<Record<string, GitDraft>> = {};
let sessions: Readonly<Record<string, GitSession>> = {};
/**
 * One component's own draft, by the key that makes it that draft — the
 * least recently touched let go past `MAX_SESSION_DRAFTS`, since a key
 * carrying a file's hash is left behind by every save.
 */
const memory = new Lru<unknown>(MAX_SESSION_DRAFTS);
const loaded = new Set<string>();
/** The scopes whose kept view was laid onto their session — once a window. */
const met = new Set<string>();
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function readDraft(scope: string): GitDraft {
  const message = readPref(webStorage(), draftStorageKey(scope), (raw) => raw, "");
  return message ? { message } : EMPTY_DRAFT;
}

function writeDraft(scope: string, draft: GitDraft): void {
  writePref(webStorage(), draftStorageKey(scope), draft.message || null);
}

/** Bring a scope's stored draft in, once. Idempotent; safe during render. */
function ensure(scope: string): void {
  if (loaded.has(scope)) return;
  loaded.add(scope);
  const stored = readDraft(scope);
  if (stored !== EMPTY_DRAFT) drafts = { ...drafts, [scope]: stored };
}

// ---------------------------------------------------------------------------
// Drafts
// ---------------------------------------------------------------------------

export function setMessage(scope: string, message: string): void {
  ensure(scope);
  const next: GitDraft = message ? { message } : EMPTY_DRAFT;
  if ((drafts[scope] ?? EMPTY_DRAFT).message === next.message) return;
  drafts = { ...drafts, [scope]: next };
  writeDraft(scope, next);
  emit();
}

export function useGitDraft(scope: string): GitDraft {
  ensure(scope);
  return useSyncExternalStore(
    subscribe,
    () => drafts[scope] ?? EMPTY_DRAFT,
    () => EMPTY_DRAFT,
  );
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/**
 * Bring how a scope's panel stood back in, once: the kept view on a clean
 * session. Idempotent and told to nobody — safe during render, and the
 * session it makes is the one every read of the scope is handed.
 */
function meet(scope: string): void {
  if (met.has(scope)) return;
  met.add(scope);
  if (sessions[scope]) return;
  const stood = withGitView(EMPTY_SESSION, viewState.read(gitPlace(scope), VIEW));
  if (stood !== EMPTY_SESSION) sessions = remember(sessions, scope, stood);
}

export function sessionFor(scope: string): GitSession {
  meet(scope);
  return sessions[scope] ?? EMPTY_SESSION;
}

/** Merge a patch into a scope's session — the one write every operation uses. */
export function patchSession(scope: string, patch: Partial<GitSession> | ((s: GitSession) => GitSession)): void {
  const current = sessionFor(scope);
  const next = typeof patch === "function" ? patch(current) : { ...current, ...patch };
  if (next === current) return;
  sessions = remember(sessions, scope, next);
  // How the panel stands is kept for the next window; what an operation left is not.
  if (!sameGitView(current, next)) {
    const view = gitViewOf(next);
    viewState.keepQuietly(gitPlace(scope), VIEW, sameKept(view, FRESH_VIEW) ? null : view);
  }
  emit();
}

/**
 * A write answered with fresh rows — a stage, a hunk, a conflict settled —
 * from the panel or from a patch document: the rows are taken, the
 * selection follows its file to the side that still has a patch, the last
 * failure is cleared, and every mounted reader is told to reload.
 */
export function filesLanded(scope: string, files: GitFileRow[]): void {
  patchSession(scope, (s) => ({ ...s, files, selection: keepSelection(s.selection, files), failure: null, stale: s.stale + 1 }));
  landedGitFiles(scope, files);
}

export function useGitSession(scope: string): GitSession {
  meet(scope);
  return useSyncExternalStore(
    subscribe,
    () => sessions[scope] ?? EMPTY_SESSION,
    () => EMPTY_SESSION,
  );
}

// ---------------------------------------------------------------------------
// One component's own draft, in the memory tier
// ---------------------------------------------------------------------------

function setMemoryDraft<T>(key: string, value: T): void {
  if (memory.get(key) === value) return;
  memory.set(key, value);
  emit();
}

/**
 * A session draft written from outside a component's render — the door that
 * opens a file *for review* forcing its mode and its lens before the
 * document mounts (`reviewLensModel.reviewOpenDrafts`). A mounted document
 * reading the same key follows at once.
 */
export function writeSessionDraft<T>(key: string, value: T): void {
  setMemoryDraft(key, value);
}

/**
 * `useState` that a tab switch cannot reach: the value lives in the store
 * under `key`, seeded with `initial` the first time the key is seen. Pass a
 * key that names *this* draft — the scope, the file, its content hash — so a
 * different file or a file that moved on is a different draft. A draft not
 * touched for `MAX_SESSION_DRAFTS` others is let go, and reads as `initial`.
 */
export function useSessionDraft<T>(key: string, initial: T): [T, (next: T | ((prev: T) => T)) => void] {
  // Nothing is written during render: an unseen key reads as `initial`
  // (pass a stable one) and the first `set` is what stores it.
  const value = useSyncExternalStore(
    subscribe,
    () => (memory.has(key) ? (memory.get(key) as T) : initial),
    () => initial,
  );
  const set = (next: T | ((prev: T) => T)) => {
    const prev = memory.has(key) ? (memory.get(key) as T) : initial;
    setMemoryDraft(key, typeof next === "function" ? (next as (p: T) => T)(prev) : next);
  };
  return [value, set];
}

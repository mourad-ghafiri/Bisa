/**
 * The rules behind the git panel store (ide/04 §What survives a switch): the
 * empty session, how a write's answer lands, what a failure remembers so it
 * can be retried after the panel that met it is gone, and the cap on how many
 * checkouts' sessions are kept in memory. Pure, so `node --test` reads them;
 * `gitPanelStore.ts` holds the state and `gitOps.ts` runs the operations.
 *
 * Two tiers, one rule each. A **draft** is what the person typed — the
 * commit message — and is persisted as `bisa:git:<scope>`, removed when
 * it empties. A **session** is what this app run's operations left behind —
 * busy, the note under Suggest, the pull banner, the publish report, the last
 * failure — and lives in memory only: restoring *Pushing…* from a previous
 * run would be a lie, so a restart starts clean while a tab switch keeps all.
 *
 * One part of a session is neither: **how the panel stood** — the file
 * selected, the rows folded shut, the history's filter (`gitViewOf`). That
 * is the person's view, not an operation's leavings, so it is kept across a
 * restart (`crates/desktop.md` §Per-viewer state) and laid back onto a clean
 * session (`withGitView`), made safe first (`parseGitView`).
 */

import { wordOf, wordsValue } from "../../shell/viewValuesModel.mjs";
import { keepSelection } from "./gitFiles.mjs";

/** How many checkouts' sessions are remembered; the oldest touched goes first. */
export const MAX_SESSIONS = 32;

/**
 * How many of one component's own drafts the memory tier keeps
 * (`useSessionDraft`): a branch name typed in a dialog, a conflict's merged
 * text under the file's hash, a hunk's picked lines, a document's mode. Each
 * is keyed by what makes it *this* draft — a file's content hash among them
 * — so a session that edits, saves and edits again leaves a key behind each
 * time; the least recently touched goes past this many, and a component
 * reading it starts from its initial value again.
 */
export const MAX_SESSION_DRAFTS = 512;

export const EMPTY_DRAFT = Object.freeze({ message: "" });

export const EMPTY_SESSION = Object.freeze({
  busy: null,
  amend: false,
  note: null,
  failure: null,
  pending: null,
  selection: null,
  stashing: null,
  shownStash: null,
  files: null,
  pullBanner: null,
  publish: Object.freeze({ kind: "none" }),
  prPublish: Object.freeze({ kind: "none" }),
  prOpen: false,
  afterMerge: null,
  stale: 0,
  graphRefs: "all",
  changesFolds: Object.freeze([]),
  operation: null,
});

/** The history's one filter, as its words. */
const GRAPH_REFS = Object.freeze(["all", "head"]);
const graphRefsValue = wordOf(GRAPH_REFS);

/**
 * The view part of a session — how the panel stood: what a restart keeps.
 * Everything else a session holds is this app run's, and starts clean.
 * @param {{selection: {path: string, staged: boolean} | null, changesFolds: readonly string[], graphRefs: string}} session
 * @returns {{selection: {path: string, staged: boolean} | null, changesFolds: string[], graphRefs: string}}
 */
export function gitViewOf(session) {
  return {
    selection: session.selection ? { path: session.selection.path, staged: session.selection.staged } : null,
    changesFolds: [...session.changesFolds],
    graphRefs: session.graphRefs,
  };
}

/** Whether two sessions stand the same — by identity, part by part: a session that only got busy has nothing new to keep. */
export function sameGitView(a, b) {
  return a.selection === b.selection && a.changesFolds === b.changesFolds && a.graphRefs === b.graphRefs;
}

/**
 * A view read back from a memory that outlives the window: a record with a
 * selection that names a path and a side, folds that are words, a filter
 * the history offers — each part that is not is the clean session's — else
 * nothing: a memory written by hand or by another version is no view.
 * @param {unknown} raw
 * @returns {{selection: {path: string, staged: boolean} | null, changesFolds: string[], graphRefs: string} | null}
 */
export function parseGitView(raw) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const at = raw.selection;
  const selection = at && typeof at === "object" && typeof at.path === "string" && at.path.length > 0 && typeof at.staged === "boolean" ? { path: at.path, staged: at.staged } : null;
  return {
    selection,
    changesFolds: wordsValue(raw.changesFolds) ?? [],
    graphRefs: graphRefsValue(raw.graphRefs) ?? EMPTY_SESSION.graphRefs,
  };
}

/**
 * A session with a kept view laid onto it: the view part is the kept one,
 * the rest is the session's — clean, on a fresh one. What is no view leaves
 * the session as it is, by identity.
 * @template S
 * @param {S} session
 * @param {unknown} kept
 * @returns {S}
 */
export function withGitView(session, kept) {
  const view = parseGitView(kept);
  return view ? { ...session, ...view } : session;
}

/**
 * A write's answer landed: the fresh rows are shown, the selection follows
 * them, the last failure is cleared, and mounted readers reload their reads.
 * @template S
 * @param {S & { selection: { path: string; staged: boolean } | null; stale: number }} session
 * @param {readonly import("../../types").GitFileRow[]} files
 */
export function applied(session, files) {
  return {
    ...session,
    files,
    selection: keepSelection(session.selection, files),
    failure: null,
    stale: session.stale + 1,
  };
}

/**
 * What a failed write remembers — never a closure, so the panel that meets
 * it later can rebuild the retry.
 * @param {"stage" | "unstage" | "discard" | "delete" | "commit" | "suggest" | "stash" | "stash_apply" | "stash_pop" | "stash_drop" | "branch"} kind
 * @param {string} error
 * @param {readonly string[]} [paths]
 */
export function failureOf(kind, error, paths = []) {
  return { kind, error, paths: [...paths] };
}

/**
 * Keep at most `max` sessions, the most recently touched last.
 * @template T
 * @param {Readonly<Record<string, T>>} byScope
 * @param {string} touched the scope just written
 * @param {T} value
 * @param {number} [max]
 */
export function remember(byScope, touched, value, max = MAX_SESSIONS) {
  const entries = Object.entries(byScope).filter(([k]) => k !== touched);
  entries.push([touched, value]);
  return Object.fromEntries(entries.slice(-max));
}

/**
 * The key a per-file draft is kept under — a conflict's merged text, a hunk's
 * picks — so a different file, side or content is a different draft.
 * @param {string} scope
 * @param {string} path
 * @param {string} facet `conflict:<hash>`, `hunks:<staged>:<hash>`, …
 */
export function fileDraftKey(scope, path, facet) {
  return `${scope}|${path}|${facet}`;
}

/**
 * A short fingerprint of a text — the facet of a per-file draft key, so a
 * patch or a note body that changed on the node is a different draft and the
 * old one is left behind. Not a hash anyone compares across machines.
 * @param {string} text
 */
export function fingerprint(text) {
  let h = 5381;
  for (let i = 0; i < text.length; i++) h = ((h << 5) + h + text.charCodeAt(i)) | 0;
  return `${text.length}:${(h >>> 0).toString(36)}`;
}

/** The `localStorage` key a scope's draft is kept under. */
export function draftStorageKey(scope) {
  return `bisa:git:${scope}`;
}

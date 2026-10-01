/**
 * Which documents are open, per root. One module variable, shared by every
 * mount, the same shape as `shell/useTerminals.ts`.
 *
 * **`snapshot()` returns the state and nothing else.** React 19 throws *"The
 * result of getSnapshot should be cached to avoid an infinite loop"* the moment
 * it returns a freshly-built object, and the obvious convenience here —
 * returning `tabsFor(state, key)`, which allocates because it prepends About —
 * is exactly that mistake. Callers take the raw state and derive in a `useMemo`.
 *
 * Nothing here persists to `localStorage`. A restored tab list pointing at
 * files that were deleted while the app was closed is a worse first impression
 * than an empty workbench, and the URL already restores the one document that
 * mattered.
 *
 * A root that is gone for good — its workstream closed, its project deleted
 * — takes what was kept of its view with it (`forgetRootMemory`): its own
 * place, its Git panel's, and every document's view. The memories outlive
 * the window; the root does not. Leaving a root is not its end: its
 * documents close (`forgetWorkbenchRoot`) and how it stood is kept.
 */

import { useSyncExternalStore } from "react";
import {
  activateDoc,
  closeAllTabs,
  closeDocPane,
  closeOtherTabs,
  closeSavedTabs,
  closeTab,
  closeTabsRight,
  closedTabs,
  emptyWorkbench,
  focusDocPane,
  forgetRoot,
  heldRoots,
  keepTab,
  moveDocToPane,
  moveTab,
  nextUntitledSeq,
  openTab,
  openTabBeside,
  reopenLastClosed,
  replaceTab,
  retargetTabs,
  setDocPaneRatio,
  splitDocPane,
  tabId,
  tabsFor,
  togglePin,
  reconcileStrip,
} from "./workbenchModel.mjs";
import type { WorkbenchState, WorkbenchTab } from "./workbenchModel.mjs";
import type { PaneNode } from "../../shell/paneTreeModel.mjs";
import { forgetBuffers, moveBuffers, unsavedNow } from "./docBuffersStore";
import { forgetRootViews, forgetViews, moveViews } from "./docViewStore";
import { editorKey } from "./editorRegistry";
import { placesOfRoot } from "./idePlacesModel.mjs";
import { viewState } from "../../shell/viewMemoryStore";

let state: WorkbenchState = emptyWorkbench();
const listeners = new Set<() => void>();

function set(next: WorkbenchState) {
  if (next === state) return;
  // Every change to the tabs comes through here, so this is the one place a
  // closed tab's buffer and its place are forgotten — a close, a replace, a
  // root past the cap — and no close path can leave one behind or take one
  // by mistake.
  const gone = closedTabs(state, next).map(([root, id]) => editorKey(root, id));
  state = next;
  if (gone.length > 0) {
    forgetBuffers(gone);
    forgetViews(gone);
  }
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function snapshot(): WorkbenchState {
  return state;
}

export function useWorkbench(): WorkbenchState {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** The roots with documents open, most recently used first — where a loose file lands when no workbench is on screen. */
export function workbenchRootKeys(): string[] {
  return state.roots.map((r) => r.key);
}

/**
 * The roots the cap must not take (`workbenchModel.capRoots`): the ones
 * holding unsaved work. A root that goes takes its tabs and their buffers
 * with it, so one with text nobody saved stays however many were opened
 * after it.
 */
function held(): string[] {
  return heldRoots(unsavedNow());
}

/** Open a document — kept, or as the focused pane's preview (`workbenchModel.mjs` § Preview tabs). */
export function openDoc(key: string, tab: WorkbenchTab, opts: { preview?: boolean } = {}): void {
  set(openTab(state, key, tab, { preview: opts.preview, held: held() }));
}

/** Open a document beside what is open (ide/19): a lone pane is split and the document takes the new half. */
export function openDocBeside(key: string, tab: WorkbenchTab): void {
  set(openTabBeside(state, key, tab, "row", held()));
}

/** ⌘N: a new untitled document in this root, kept, numbered after the ones open. Returns its tab. */
export function openUntitledDoc(key: string): WorkbenchTab {
  const tab: WorkbenchTab = { kind: "untitled", seq: nextUntitledSeq(tabsFor(state, key)) };
  set(openTab(state, key, tab, { held: held() }));
  return tab;
}

/** An untitled document was saved under a name: its tab becomes the file's, in place. */
export function replaceDoc(key: string, fromId: string, tab: WorkbenchTab): void {
  set(replaceTab(state, key, fromId, tab));
}

/** A preview becomes a kept tab; nothing happens to a kept one. */
export function keepDoc(key: string, id: string): void {
  set(keepTab(state, key, id));
}

/** *Close saved*: every document that is neither dirty nor pinned. */
export function closeSavedDocs(key: string, dirtyIds: Iterable<string>): void {
  set(closeSavedTabs(state, key, dirtyIds));
}

export function closeDoc(key: string, id: string): void {
  set(closeTab(state, key, id));
}

export function closeOtherDocs(key: string, id: string): void {
  set(closeOtherTabs(state, key, id));
}

export function closeAllDocs(key: string): void {
  set(closeAllTabs(state, key));
}

/** For a root that has been deleted out from under the workbench, or left: its documents close. */
export function forgetWorkbenchRoot(key: string): void {
  set(forgetRoot(state, key));
}

/**
 * For a root that is gone for good — its workstream closed or retired, its
 * project deleted: what was kept of how it stood goes with it — the root's
 * own place, its Git panel's, and the view of every document of it, open
 * or not.
 */
export function forgetRootMemory(key: string): void {
  forgetRootViews(key);
  for (const place of placesOfRoot(key)) viewState.forget(place);
}

/** A rename on disk: the tabs at or under `from` follow to `to`. Returns what moved. */
export function retargetDocs(key: string, from: string, to: string): [string, string][] {
  const next = retargetTabs(state, key, from, to);
  // Before the tabs change: a buffer and a place follow their file to the
  // new path rather than being forgotten with the old tab id.
  const moves = next.moved.map(([was, now]) => ({
    from: editorKey(key, tabId({ kind: "file", path: was })),
    to: editorKey(key, tabId({ kind: "file", path: now })),
    path: now,
  }));
  moveBuffers(moves);
  moveViews(moves);
  set(next.state);
  return next.moved;
}

/** Show a document in its pane and focus that pane — the URL's active tab, mirrored. */
export function activateDocTab(key: string, id: string): void {
  set(activateDoc(state, key, id));
}
export function splitDocs(key: string, dir: "row" | "col", leafId?: string | null): void {
  set(splitDocPane(state, key, dir, leafId ?? null));
}
export function closeDocsPane(key: string, leafId: string): void {
  set(closeDocPane(state, key, leafId));
}
export function moveDoc(key: string, id: string, leafId: string): void {
  set(moveDocToPane(state, key, id, leafId));
}
export function focusDocs(key: string, leafId: string): void {
  set(focusDocPane(state, key, leafId));
}
export function setDocsPaneRatio(key: string, splitId: string, ratio: number): void {
  set(setDocPaneRatio(state, key, splitId, ratio));
}
export function togglePinnedDoc(key: string, id: string): void {
  set(togglePin(state, key, id));
}
/** A drag along the strip it was shown on; the refusal, if any, is the words to show. */
export function moveDocTab(key: string, id: string, index: number, shown?: readonly string[]): string | null {
  const next = moveTab(state, key, id, index, shown ?? null);
  set(next.state);
  return next.refused;
}

/** The strip learns what it drew (`reconcileStrip`) — called once per render of the merged list. */
export function reconcileStripOrder(key: string, ids: readonly string[]): void {
  set(reconcileStrip(state, key, ids, held()));
}
export function closeDocsRight(key: string, id: string): void {
  set(closeTabsRight(state, key, id));
}

/**
 * A saved layout comes back whole: its documents, its pane tree and its pins,
 * in one step — so the tree is never rebuilt tab by tab into one pane first.
 */
export function restoreDocs(key: string, tabs: WorkbenchTab[], panes: PaneNode, pinned: string[], strip: readonly string[] = []): void {
  let next = state;
  const keep = held();
  for (const tab of tabs) next = openTab(next, key, tab, { held: keep });
  const entry = next.roots.find((r) => r.key === key);
  if (!entry) return;
  // The saved order first, then whatever the restore opened that it did not name.
  const known = new Set(strip);
  const order = [...strip, ...entry.strip.filter((id) => !known.has(id))];
  set({ roots: next.roots.map((r) => (r.key === key ? { ...entry, panes, pinned: pinned.filter((p) => entry.tabs.some((t) => tabId(t) === p)), strip: order } : r)) });
}

/** Reopen the last document closed in this root; the tab to activate, or null. */
export function reopenLastClosedDoc(key: string): WorkbenchTab | null {
  const next = reopenLastClosed(state, key);
  set(next.state);
  return next.tab;
}

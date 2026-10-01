import type { WorkbenchScope } from "../../router";
import type { PaneNode } from "../../shell/paneTreeModel.mjs";

export type WorkbenchTab =
  | { kind: "file"; path: string }
  | { kind: "diff" }
  /** One changed file's patch on one side — the index against HEAD (`staged`) or the tree against the index (ide/04). */
  | { kind: "patch"; path: string; staged: boolean }
  /** One commit, whole — message, refs, files and patch (ide/05). */
  | { kind: "commit"; sha: string }
  /** An artifact of a message, as a document (ide/12); the title is the tab's word. */
  | { kind: "artifact"; message: string; ordinal: number; title: string }
  /** A session's transcript as a document (ide/09); the title is the agent or the harness. */
  | { kind: "transcript"; session: string; title: string }
  /** An untitled document (⌘N): a buffer with no file behind it until it is saved under a name. */
  | { kind: "untitled"; seq: number }
  /** A loose file: one from anywhere on this machine, by its canonical absolute path, under no root (ide/03 §Loose files). */
  | { kind: "loose"; path: string }
  /** A device mirrored beside the code (ide/19): a simulator, an emulator or a phone, by the id the node lists it under. */
  | { kind: "device"; id: string }
  /** A terminal session's tab — never stored here; see `mergedTabs`. */
  | { kind: "terminal"; key: string }
  /** A browser tab (ide/18) — the browser store's, never stored here; see `mergedTabs`. */
  | { kind: "browser"; key: string };

export interface RootTabs {
  readonly key: string;
  readonly tabs: WorkbenchTab[];
  /** Documents closed here, most recent first, capped at `MAX_RECENTLY_CLOSED`. */
  readonly recentlyClosed: WorkbenchTab[];
  /** The pane tree over the document tab ids; one leaf until somebody splits. */
  readonly panes: PaneNode;
  readonly paneSeq: number;
  readonly focusedPane: string;
  /** Tab ids pinned in this root. */
  readonly pinned: string[];
  /** Each pane's one preview tab, by leaf id. */
  readonly preview: Readonly<Record<string, string>>;
  /** The strip's one order — document ids and terminal ids, as opened or dragged, newest last. */
  readonly strip: string[];
}

export interface WorkbenchState {
  /** Most recently opened-in first; capped at `MAX_ROOTS`. */
  readonly roots: RootTabs[];
}

export declare const WORKBENCH_SCOPES: readonly WorkbenchScope[];
export declare const MAX_ROOTS: number;
export declare const MAX_RECENTLY_CLOSED: number;

export declare function emptyWorkbench(): WorkbenchState;
export declare function rootKey(scope: WorkbenchScope, id: string): string;
export declare function tabId(tab: WorkbenchTab | null | undefined): string;
/** What an untitled document is called until it is saved: `Untitled-<seq>`. */
export declare function untitledName(seq: number): string;
/** The lowest number no untitled tab of the root holds. */
export declare function nextUntitledSeq(tabs: readonly WorkbenchTab[] | null | undefined): number;
/** One tab becomes another in place — its position, its pane and its pin kept. */
export declare function replaceTab(state: WorkbenchState, key: string, fromId: string, tab: WorkbenchTab): WorkbenchState;
export declare function parseTabId(id: string | null | undefined): WorkbenchTab | null;
export declare function tabIn(docs: readonly WorkbenchTab[], id: string | null | undefined): WorkbenchTab | null;
export declare function tabsFor(state: WorkbenchState, key: string): WorkbenchTab[];
export declare function mergedTabs(docs: readonly WorkbenchTab[], terminalSessions: readonly { key: string }[] | null | undefined, strip?: readonly string[], browserSessions?: readonly { key: string }[] | null): WorkbenchTab[];
/** One pane's rendered strip: its documents and, for the first pane, the terminals, in the root's order, pinned first. */
export declare function leafStrip(strip: readonly string[], leafTabs: readonly string[], terminalIds: readonly string[] | null | undefined, pinned: readonly string[] | null | undefined): string[];
/** The strip learns what it drew: unseen ids recorded at the end, gone ones forgotten; the same state when nothing moved. */
export declare function reconcileStrip(state: WorkbenchState, key: string, ids: readonly string[], held?: Iterable<string> | null): WorkbenchState;
/** The roots that hold unsaved work, from the keys of the buffers that do. */
export declare function heldRoots(unsaved: Iterable<string> | null | undefined): string[];
/** The roots kept: the first `MAX_ROOTS`, and past them every root holding unsaved work. */
export declare function capRoots<R extends { key: string }>(roots: readonly R[], held: Iterable<string> | null | undefined): readonly R[];
export declare function openTab(
  state: WorkbenchState,
  key: string,
  tab: WorkbenchTab,
  /** `held`: the roots holding unsaved work — the cap never takes one. */
  opts?: { preview?: boolean; held?: Iterable<string> | null },
): WorkbenchState;
export declare function keepTab(state: WorkbenchState, key: string, id: string): WorkbenchState;
export declare function isPreview(state: WorkbenchState, key: string, id: string): boolean;
export declare function previewIds(state: WorkbenchState, key: string): string[];
export declare function savedTabs(state: WorkbenchState, key: string, dirtyIds: Iterable<string>): string[];
export declare function closeSavedTabs(state: WorkbenchState, key: string, dirtyIds: Iterable<string>): WorkbenchState;
export declare function closeTab(state: WorkbenchState, key: string, id: string): WorkbenchState;
export declare function closeOtherTabs(
  state: WorkbenchState,
  key: string,
  id: string,
): WorkbenchState;
export declare function closeAllTabs(state: WorkbenchState, key: string): WorkbenchState;
export declare function forgetRoot(state: WorkbenchState, key: string): WorkbenchState;
export declare function tabsUnder(tabs: readonly WorkbenchTab[], path: string): WorkbenchTab[];
export declare function panesFor(state: WorkbenchState, key: string): PaneNode;
export declare function splitDocPane(state: WorkbenchState, key: string, dir: "row" | "col", leafId?: string | null): WorkbenchState;
/** Open a document beside what is open: one pane with something in it is split and the document goes into the new half (ide/19). */
export declare function openTabBeside(state: WorkbenchState, key: string, tab: WorkbenchTab, dir?: "row" | "col", held?: Iterable<string> | null): WorkbenchState;
export declare function closeDocPane(state: WorkbenchState, key: string, leafId: string): WorkbenchState;
export declare function moveDocToPane(state: WorkbenchState, key: string, id: string, leafId: string): WorkbenchState;
export declare function focusDocPane(state: WorkbenchState, key: string, leafId: string): WorkbenchState;
export declare function activateDoc(state: WorkbenchState, key: string, id: string): WorkbenchState;
export declare function setDocPaneRatio(state: WorkbenchState, key: string, splitId: string, ratio: number): WorkbenchState;
export declare function isPinned(state: WorkbenchState, key: string, id: string): boolean;
export declare function togglePin(state: WorkbenchState, key: string, id: string): WorkbenchState;
export declare function stripOrder(tabIds: readonly string[], pinned: readonly string[] | null | undefined): string[];
export declare function moveTab(state: WorkbenchState, key: string, id: string, index: number, shown?: readonly string[] | null): { state: WorkbenchState; refused: string | null };
export declare function closeTabsRight(state: WorkbenchState, key: string, id: string): WorkbenchState;
export declare function tabsRightOf(state: WorkbenchState, key: string, id: string): string[];
export declare function closedTabs(before: WorkbenchState, after: WorkbenchState): [string, string][];
export declare function retargetTabs(
  state: WorkbenchState,
  key: string,
  from: string,
  to: string,
): { state: WorkbenchState; moved: [string, string][] };
export declare function reopenLastClosed(
  state: WorkbenchState,
  key: string,
): { state: WorkbenchState; tab: WorkbenchTab | null };
export declare function nextActiveAfterClose(
  tabs: WorkbenchTab[],
  closingId: string,
  activeId: string | null,
): string | null;
export declare function tabLabel(tab: WorkbenchTab | null | undefined): string;
export declare function tabTitle(tab: WorkbenchTab | null | undefined): string;

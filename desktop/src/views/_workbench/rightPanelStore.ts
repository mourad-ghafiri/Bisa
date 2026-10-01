/**
 * The right panel (layout's rail): whether its column is
 * showing, which occupant, the root it is showing it for, which occupant
 * each root last had, and — for the occupants that have views of their own
 * (Git, About) — the view each is on. Window furniture, in `localStorage`.
 * The vocabulary — what may occupy the panel, how the rail groups it, which
 * views an occupant has — is `rightPanelModel.mjs`; this file only keeps the
 * state.
 *
 * One module store rather than component state because the keymap opens an
 * occupant from outside the workbench (⌘⇧G is "the Git panel", from anywhere
 * in a root), the terminal strip's *Send to agent* opens Agents, and a
 * refused push's banner opens About on its Settings view.
 */

import { useSyncExternalStore } from "react";
import { jsonPref, readPref, switchWord, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { parseRememberedTabs, parseViews, pressOccupant as pressRule, recallFor } from "./rightPanelModel.mjs";
import type { Occupant, PanelViews, ViewChoice, ViewedOccupant } from "./rightPanelModel.mjs";

export type { AboutView, ChangesFilter, ChangesLayout, GitView, Occupant, PanelViews, RemoteLayout, ViewChoice, ViewedOccupant } from "./rightPanelModel.mjs";

interface State {
  /** Whether the panel's column is showing. The rail always is. */
  readonly open: boolean;
  /** The occupant showing now — or the one the column would show when opened. */
  readonly tab: Occupant;
  /** The root the panel is showing for — what `recallRightTab` compares against. */
  readonly root: string | null;
  /** Per-root memory of the last occupant, capped. */
  readonly byRoot: Readonly<Record<string, Occupant>>;
  /** The view each viewed occupant is on — one answer for every root. */
  readonly views: Readonly<PanelViews>;
}

const OPEN_KEY = "bisa.ide.right.open";
const TABS_KEY = "bisa.ide.right.tabs";
const VIEWS_KEY = "bisa.ide.views";
const MAX_REMEMBERED = 32;

let state: State = {
  open: readPref(webStorage(), OPEN_KEY, (raw) => raw !== "0", true),
  tab: "files",
  root: null,
  byRoot: readPref(webStorage(), TABS_KEY, (raw) => parseRememberedTabs(jsonPref(raw)), {}),
  views: readPref(webStorage(), VIEWS_KEY, (raw) => parseViews(jsonPref(raw)), parseViews(null)),
};
const listeners = new Set<() => void>();

export function useRightPanel(): State {
  return useSyncExternalStore(subscribe, () => state, () => state);
}

/** One remembered choice — an occupant's view, the Changes layout — as a subscription. */
export function usePanelView<C extends ViewChoice>(choice: C): PanelViews[C] {
  return useSyncExternalStore(subscribe, () => state.views[choice], () => state.views[choice]);
}

/**
 * Show the panel on an occupant, remembering it for the root (the one given,
 * else the root the panel is showing for). A chord and every "open X" door
 * in the app come through here: it opens, never closes.
 */
export function showRightPanel(tab: Occupant, root?: string | null): void {
  const at = root ?? state.root;
  const byRoot = at ? remember(state.byRoot, at, tab) : state.byRoot;
  if (state.open && state.tab === tab && byRoot === state.byRoot) return;
  set({ ...state, open: true, tab, byRoot });
}

/**
 * A rail icon pressed: open on it, switch to it, or — when it is already
 * showing — close the column. The rule is `pressOccupant`'s; the occupant is
 * remembered for the root either way, so reopening lands where you were.
 */
export function pressOccupant(tab: Occupant): void {
  const next = pressRule(state, tab);
  const byRoot = state.root ? remember(state.byRoot, state.root, next.tab) : state.byRoot;
  set({ ...state, ...next, byRoot });
}

/**
 * Put the panel on an occupant without opening or closing it — for a
 * wanted occupant the centre answered instead (ide/09, Agent Mode), so the
 * column shows what it can and remembers that for the root.
 */
export function settleOccupant(tab: Occupant, root?: string | null): void {
  const at = root ?? state.root;
  const byRoot = at ? remember(state.byRoot, at, tab) : state.byRoot;
  if (state.tab === tab && byRoot === state.byRoot) return;
  set({ ...state, tab, byRoot });
}

export function setRightPanelOpen(open: boolean): void {
  if (open === state.open) return;
  set({ ...state, open });
}

export function toggleRightPanel(): void {
  set({ ...state, open: !state.open });
}

/** Called when the workbench's root changes: recall that root's occupant. */
export function recallRightTab(root: string): void {
  const next = recallFor(state, root);
  if (next.tab === state.tab && next.root === state.root) return;
  set({ ...state, ...next });
}

/** Remember one choice — put an occupant on one of its views, or the Changes view on a layout. The panel is not opened: pair it with `showRightPanel`. */
export function setPanelView<C extends ViewChoice>(choice: C, view: PanelViews[C]): void {
  if (view === state.views[choice]) return;
  set({ ...state, views: { ...state.views, [choice]: view } });
}

/** Open the panel on an occupant **and** on one of its views — the shape of every door into a view. */
export function openPanelView<O extends ViewedOccupant>(occupant: O, view: PanelViews[O], root?: string | null): void {
  setPanelView(occupant, view);
  showRightPanel(occupant, root);
}

function remember(byRoot: Readonly<Record<string, Occupant>>, root: string, tab: Occupant) {
  if (byRoot[root] === tab) return byRoot;
  const entries = Object.entries(byRoot).filter(([k]) => k !== root);
  entries.push([root, tab]);
  return Object.fromEntries(entries.slice(-MAX_REMEMBERED));
}

function set(next: State): void {
  state = next;
  const storage = webStorage();
  writePref(storage, OPEN_KEY, switchWord(next.open));
  writePref(storage, TABS_KEY, next.byRoot);
  writePref(storage, VIEWS_KEY, next.views);
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

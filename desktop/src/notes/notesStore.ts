/**
 * Whether the notes overlay is open, whether it is maximized, where its dock
 * sits, and which note is on screen. One module variable, shared by every mount, the same shape as
 * `views/_workbench/workbenchStore.ts` and `shell/useTerminals.ts`.
 *
 * **`snapshot()` returns the state and nothing else** — React 19 throws *"The
 * result of getSnapshot should be cached to avoid an infinite loop"* the
 * moment it returns a freshly-built object.
 *
 * A module store rather than context, for the reason `useTerminals` gives:
 * the things that open a note sit at different depths under different
 * screens, and threading a provider to all of them would put "is it open, and
 * where" in the hands of whoever wires the next one.
 *
 * **The panel keeps its place.** Which tab it lists and which note is open
 * change only through this store's own doors — a hand on the tab strip, a
 * click on a row, Back — and never because the route moved: the reason it is
 * an overlay is that you can read a note about one thing while looking at
 * another. The tab persists (a preference about how you use the panel). The
 * open note and the list's search are how the panel stood: they are kept
 * under the place `notes` in the view memory (`shell/viewMemoryStore.ts`),
 * quietly, and read once when this store loads — so a restart comes back to
 * the note that was open. A note that went while the app was closed opens
 * nothing (`settleRestoredNote`). A note's unsaved words are its draft's,
 * kept elsewhere; nothing of them is here.
 */

import { useSyncExternalStore } from "react";
import { createLeaveGuard } from "../shell/documentGuard";
import { forgetPref, jsonPref, readPref, switchWord, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import { viewState } from "../shell/viewMemoryStore";
import { idValue, textValue } from "../shell/viewValuesModel.mjs";
import { placementFrom, samePlacement, type Placement } from "../ui/Dock";
import {
  SAVE_DEFAULT_MS,
  clampSaveDelay,
  listedNote,
  noteTab,
  noteView,
  type NoteTab,
  type NoteView,
} from "./notesModel.mjs";

const OPEN_KEY = "bisa.notes.open";
const TAB_KEY = "bisa.notes.tab";
const DOCK_KEY = "bisa.notes.dock";
const DOCK_VISIBLE_KEY = "bisa.notes.dockVisible";
const COUNT_BADGE_KEY = "bisa.notes.countBadge";
const DEFAULT_VIEW_KEY = "bisa.notes.defaultView";
const SAVE_DELAY_KEY = "bisa.notes.saveDelay";
const MAXIMIZED_KEY = "bisa.notes.maximized";

/** The place the panel keeps how it stood under, and the two names in it. */
const PLACE = "notes";
const ACTIVE = "active";
const QUERY = "query";

/** Where the dock starts before anyone has dragged it: 24 in from the bottom-right corner. */
const DEFAULT_DOCK: Placement = { h: "right", x: 24, v: "bottom", y: 24 };

/**
 * The overlay's window state, and the preferences that shape it.
 *
 * **Flat fields, not a nested `prefs` object.** `snapshot()` has to return the
 * same reference until something actually changes, and a sub-object rebuilt
 * per read is exactly the React 19 *"The result of getSnapshot should be
 * cached"* loop this file's header warns about. Grouping them would buy
 * tidiness in the type and cost the one invariant the store exists to hold.
 */
interface NotesState {
  open: boolean;
  /** Anchored to its nearest edges, so it keeps its place when the window changes size. */
  dock: Placement;
  /** The note being edited, or null for the list. */
  active: string | null;
  /** Which notes the list shows. *All* until a hand changes it. */
  tab: NoteTab;
  /** The list's search — kept across a restart, as the open note is. */
  query: string;
  /**
   * Whether the corner button is drawn at all.
   *
   * Turning it off does **not** turn off notes: `Alt+N` still opens the panel,
   * which is why this is "show the dock" and not "enable notes". A switch that
   * silently removed the only way into a feature would be a trap.
   */
  dockVisible: boolean;
  /** Whether the dock paints its note count. The accessible name keeps it either way. */
  countBadge: boolean;
  /** Whether the conversation drawer stands beside the open note. Session-only. */
  askOpen: boolean;
  /** Which view a note *opens* in. Switching inside the editor stays session-only. */
  defaultView: NoteView;
  /** Milliseconds after the last keystroke before the editor saves. */
  saveDelay: number;
  /**
   * Whether the panel fills the content column — everything but the header,
   * the footer and the sidebar (`shell/maximizedPanel.ts`, the frame Draw's
   * panel shares). Off until a hand asks: a note opens beside the work.
   */
  maximized: boolean;
}

/**
 * A stored "1"/"0" flag, defaulting to on: every one of these defaults to the
 * visible, working state, so a webview that cannot remember still gets the
 * whole feature.
 */
function storedFlag(key: string): boolean {
  return readPref(webStorage(), key, (raw) => raw !== "0", true);
}

function storedString(key: string): string | null {
  return readPref(webStorage(), key, (raw) => raw, null);
}

/** Whether the overlay was left open — closed for a machine that never said. */
function storedOpen(): boolean {
  return readPref(webStorage(), OPEN_KEY, (raw) => raw === "1", false);
}

function storedDock(): Placement {
  // The model is the one judge of what a placement is: an earlier shape,
  // a corrupt value, a negative — all take the default, which is always
  // reachable.
  return readPref(webStorage(), DOCK_KEY, (raw) => placementFrom(jsonPref(raw), DEFAULT_DOCK), DEFAULT_DOCK);
}

let state: NotesState = {
  open: storedOpen(),
  dock: storedDock(),
  active: idValue(viewState.read(PLACE, ACTIVE)) ?? null,
  tab: noteTab(storedString(TAB_KEY)),
  query: textValue(viewState.read(PLACE, QUERY)) ?? "",
  dockVisible: storedFlag(DOCK_VISIBLE_KEY),
  countBadge: storedFlag(COUNT_BADGE_KEY),
  askOpen: false,
  // Clamped on the way *out* of storage as well as in: the vocabulary and the
  // bounds can change in a later version, and a value written by an older one
  // must not be able to select a view that no longer exists or a delay the
  // current rules would refuse.
  defaultView: noteView(storedString(DEFAULT_VIEW_KEY)),
  saveDelay: readPref(webStorage(), SAVE_DELAY_KEY, clampSaveDelay, SAVE_DEFAULT_MS),
  maximized: readPref(webStorage(), MAXIMIZED_KEY, (raw) => raw === "1", false),
};
const listeners = new Set<() => void>();
/** The note that came back from the last window, until the list has said whether it is still there. */
let restored: string | null = state.active;

function set(next: NotesState) {
  if (next === state) return;
  const was = state;
  state = next;
  // How the panel stands is kept for the next window: told to nobody, since this store is what draws it.
  if (next.active !== was.active) viewState.keepQuietly(PLACE, ACTIVE, next.active);
  if (next.query !== was.query) viewState.keepQuietly(PLACE, QUERY, next.query || null);
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function snapshot(): NotesState {
  return state;
}

export function useNotesOverlay(): NotesState {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/**
 * The leave guard: every way out of an open note — Back, the panel's ×,
 * Alt+N, the dock, another note opened over it — asks it first, and it asks
 * the person while the editor holds unsaved work (`shell/documentGuard.ts`).
 */
export const notesGuard = createLeaveGuard("note");

function showPanel(open: boolean): void {
  if (state.open === open) return;
  writePref(webStorage(), OPEN_KEY, switchWord(open));
  set({ ...state, open });
}

export function setNotesOpen(open: boolean): void {
  // Closing the panel unmounts the open note's editor: the guard first.
  if (!open && state.open && state.active !== null) notesGuard.leave(() => showPanel(false));
  else showPanel(open);
}

export function toggleNotes(): void {
  setNotesOpen(!state.open);
}

/**
 * Write one preference through to storage and the subscribers.
 *
 * One helper rather than five near-identical setters. Each of these is the
 * same three steps — bail when unchanged, persist, publish — and five copies
 * of that is five places for the bail to be forgotten, which is a render loop
 * rather than a cosmetic bug.
 */
function setPref<K extends keyof NotesState>(key: K, value: NotesState[K], storageKey: string) {
  if (state[key] === value) return;
  writePref(webStorage(), storageKey, typeof value === "boolean" ? switchWord(value) : String(value));
  set({ ...state, [key]: value });
}

export function setDockVisible(on: boolean): void {
  setPref("dockVisible", on, DOCK_VISIBLE_KEY);
}

export function setCountBadge(on: boolean): void {
  setPref("countBadge", on, COUNT_BADGE_KEY);
}

export function setNotesAskOpen(on: boolean): void {
  if (state.askOpen === on) return;
  set({ ...state, askOpen: on });
}

export function setDefaultView(view: NoteView): void {
  setPref("defaultView", noteView(view), DEFAULT_VIEW_KEY);
}

export function setSaveDelay(ms: number): void {
  setPref("saveDelay", clampSaveDelay(ms), SAVE_DELAY_KEY);
}

export function setNotesMaximized(on: boolean): void {
  setPref("maximized", on, MAXIMIZED_KEY);
}

export function toggleNotesMaximized(): void {
  setNotesMaximized(!state.maximized);
}

/** A hand on the tab strip: the one thing that changes which notes are listed. */
export function setNotesTab(tab: NoteTab): void {
  setPref("tab", noteTab(tab), TAB_KEY);
}

/** What the list's search box holds — kept with the panel's view, not with its settings. */
export function setNotesQuery(query: string): void {
  if (state.query === query) return;
  set({ ...state, query });
}

/** Whether the dock has been dragged away from where it starts. */
export function dockMoved(dock: Placement): boolean {
  return !samePlacement(dock, DEFAULT_DOCK);
}

/**
 * Put the dock back in its corner.
 *
 * The escape hatch for a dock dragged somewhere awkward — over a control it
 * covers, or into a corner the reader then forgot about. The clamp already
 * guarantees it is *on* screen; this is about it being where you expect.
 */
export function resetDockPosition(): void {
  forgetPref(webStorage(), DOCK_KEY);
  set({ ...state, dock: DEFAULT_DOCK });
}

/**
 * Move the dock to a placement the drag made.
 *
 * It arrives clamped and anchored by the model (`placementOf`, from the box
 * the pointer moved), so a placement that could strand the dock is never the
 * one that gets stored, and the store has no window to measure against.
 */
export function moveDock(next: Placement): void {
  if (samePlacement(next, state.dock)) return;
  writePref(webStorage(), DOCK_KEY, next);
  set({ ...state, dock: next });
}

export function openNote(id: string | null): void {
  if (state.active === id && state.open) return;
  const show = () => set({ ...state, active: id, open: true });
  // Another note over an open one: the one being left is asked about.
  if (state.active !== null && state.active !== id && state.open) notesGuard.leave(show);
  else show();
}

/**
 * The list has loaded: a note kept from the last window that it does not
 * hold opens nothing. No guard is asked — a note the list does not hold was
 * never drawn, so there is no editor to leave.
 */
export function settleRestoredNote(listed: readonly string[]): void {
  const active = listedNote(state.active, restored, listed);
  restored = null;
  if (active !== state.active) set({ ...state, active, askOpen: false });
}

/** Back to the list without closing the panel — Back, or the open note being deleted. */
export function clearActiveNote(): void {
  if (state.active === null) return;
  notesGuard.leave(() => set({ ...state, active: null, askOpen: false }));
}

/**
 * Whether the Draw overlay is open, where its dock sits, which drawing is on
 * the canvas, and whether the panel is maximized — one module variable,
 * shared by every mount, the shape of `notes/notesStore.ts` for the reasons
 * that file gives: the things that open a drawing sit at different depths
 * under different screens; `snapshot()` returns the state and nothing else;
 * the panel keeps its place and never follows the route.
 *
 * The tab, the maximized state, the dock and its visibility, the badge and
 * the save delay persist (preferences about how you use the panel). The open
 * drawing and the list's search are how the panel stood: they are kept under
 * the place `draw` in the view memory (`shell/viewMemoryStore.ts`), quietly,
 * and read once when this store loads — so a restart comes back to the
 * drawing that was on the canvas. One that went while the app was closed
 * opens nothing and says nothing (`restoredDrawing`). The Ask drawer is the
 * moment's.
 */

import { useSyncExternalStore } from "react";
import { createLeaveGuard } from "../shell/documentGuard";
import { forgetPref, jsonPref, readPref, switchWord, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import { viewState } from "../shell/viewMemoryStore";
import { idValue, textValue } from "../shell/viewValuesModel.mjs";
import { placementFrom, samePlacement, type Placement } from "../ui/Dock";
import { SAVE_DEFAULT_MS, clampSaveDelay, drawTab, type DrawTab } from "./drawModel.mjs";

const OPEN_KEY = "bisa.draw.open";
const TAB_KEY = "bisa.draw.tab";
const DOCK_KEY = "bisa.draw.dock";
const DOCK_VISIBLE_KEY = "bisa.draw.dockvisible";
const COUNT_BADGE_KEY = "bisa.draw.badge";
const MAXIMIZED_KEY = "bisa.draw.maximized";
const SAVE_DELAY_KEY = "bisa.draw.savedelay";

/** The place the panel keeps how it stood under, and the two names in it. */
const PLACE = "draw";
const ACTIVE = "active";
const QUERY = "query";

/** Where the dock starts before anyone has dragged it: above the notes' corner, so the two never overlap. */
const DEFAULT_DOCK: Placement = { h: "right", x: 24, v: "bottom", y: 84 };

interface DrawState {
  open: boolean;
  /** Anchored to its nearest edges, so it keeps its place when the window changes size. */
  dock: Placement;
  /** The drawing on the canvas, or null for the list. */
  active: string | null;
  /** Which drawings the list shows. *All* until a hand changes it. */
  tab: DrawTab;
  /** The list's search — kept across a restart, as the open drawing is. */
  query: string;
  /** Whether the panel fills the content column — everything but the header, the footer and the sidebar. */
  maximized: boolean;
  /** Whether the Ask drawer stands beside the canvas. Session-only. */
  askOpen: boolean;
  /**
   * Whether the corner button is drawn at all. Off does **not** turn off
   * Draw: the keymap's `toggle_draw` still opens the panel, which is why this
   * is "show the dock" and not "enable drawings".
   */
  dockVisible: boolean;
  /** Whether the dock paints its drawing count. The accessible name keeps it either way. */
  countBadge: boolean;
  /** Milliseconds after the last stroke before the canvas saves. */
  saveDelay: number;
}

function storedFlag(key: string, fallback = true): boolean {
  return readPref(webStorage(), key, (raw) => raw !== "0", fallback);
}

function storedString(key: string): string | null {
  return readPref(webStorage(), key, (raw) => raw, null);
}

function storedDock(): Placement {
  return readPref(webStorage(), DOCK_KEY, (raw) => placementFrom(jsonPref(raw), DEFAULT_DOCK), DEFAULT_DOCK);
}

let state: DrawState = {
  open: readPref(webStorage(), OPEN_KEY, (raw) => raw === "1", false),
  dock: storedDock(),
  active: idValue(viewState.read(PLACE, ACTIVE)) ?? null,
  tab: drawTab(storedString(TAB_KEY)),
  query: textValue(viewState.read(PLACE, QUERY)) ?? "",
  maximized: readPref(webStorage(), MAXIMIZED_KEY, (raw) => raw === "1", false),
  askOpen: false,
  dockVisible: storedFlag(DOCK_VISIBLE_KEY),
  countBadge: storedFlag(COUNT_BADGE_KEY),
  saveDelay: readPref(webStorage(), SAVE_DELAY_KEY, clampSaveDelay, SAVE_DEFAULT_MS),
};
const listeners = new Set<() => void>();
/** The drawing that came back from the last window, until it was read — or another was opened. */
let restored: string | null = state.active;

function set(next: DrawState) {
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

function snapshot(): DrawState {
  return state;
}

export function useDrawOverlay(): DrawState {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** The leave guard (`shell/documentGuard.ts`): every way out of an open drawing asks it first. */
export const drawGuard = createLeaveGuard("drawing");

function showPanel(open: boolean): void {
  if (state.open === open) return;
  writePref(webStorage(), OPEN_KEY, switchWord(open));
  set({ ...state, open });
}

export function setDrawOpen(open: boolean): void {
  if (!open && state.open && state.active !== null) drawGuard.leave(() => showPanel(false));
  else showPanel(open);
}

export function toggleDraw(): void {
  setDrawOpen(!state.open);
}

/** One helper rather than six near-identical setters: bail when unchanged, persist, publish. */
function setPref<K extends keyof DrawState>(key: K, value: DrawState[K], storageKey: string) {
  if (state[key] === value) return;
  writePref(webStorage(), storageKey, typeof value === "boolean" ? switchWord(value) : String(value));
  set({ ...state, [key]: value });
}

export function setDrawDockVisible(on: boolean): void {
  setPref("dockVisible", on, DOCK_VISIBLE_KEY);
}

export function setDrawCountBadge(on: boolean): void {
  setPref("countBadge", on, COUNT_BADGE_KEY);
}

export function setDrawSaveDelay(ms: number): void {
  setPref("saveDelay", clampSaveDelay(ms), SAVE_DELAY_KEY);
}

export function setDrawMaximized(on: boolean): void {
  setPref("maximized", on, MAXIMIZED_KEY);
}

export function toggleDrawMaximized(): void {
  setDrawMaximized(!state.maximized);
}

/** A hand on the tab strip: the one thing that changes which drawings are listed. */
export function setDrawTab(tab: DrawTab): void {
  setPref("tab", drawTab(tab), TAB_KEY);
}

/** What the list's search box holds — kept with the panel's view, not with its settings. */
export function setDrawQuery(query: string): void {
  if (state.query === query) return;
  set({ ...state, query });
}

export function setDrawAskOpen(on: boolean): void {
  if (state.askOpen === on) return;
  set({ ...state, askOpen: on });
}

/** Whether the dock has been dragged away from where it starts. */
export function drawDockMoved(dock: Placement): boolean {
  return !samePlacement(dock, DEFAULT_DOCK);
}

/** Put the dock back where it starts. */
export function resetDrawDockPosition(): void {
  forgetPref(webStorage(), DOCK_KEY);
  set({ ...state, dock: DEFAULT_DOCK });
}

/** Move the dock to a placement the drag made — clamped and anchored by the model already. */
export function moveDrawDock(next: Placement): void {
  if (samePlacement(next, state.dock)) return;
  writePref(webStorage(), DOCK_KEY, next);
  set({ ...state, dock: next });
}

export function openDrawing(id: string | null): void {
  if (state.active === id && state.open) return;
  const show = () => set({ ...state, active: id, open: true });
  if (state.active !== null && state.active !== id && state.open) drawGuard.leave(show);
  else show();
}

/**
 * The drawing that came back from the last window, asked for once — by the
 * read that opens it: a drawing the node no longer has then goes without a
 * word (`drawModel.goneQuietly`). After the first answer nothing is restored
 * any more: every later read is one the person asked for.
 */
export function restoredDrawing(): string | null {
  const id = restored;
  restored = null;
  return id;
}

/** Back to the list without closing the panel — Back, or the open drawing being deleted. */
export function clearActiveDrawing(): void {
  if (state.active === null) return;
  drawGuard.leave(() => set({ ...state, active: null, askOpen: false }));
}

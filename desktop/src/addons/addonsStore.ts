/**
 * Which addons are installed, which show, and where each window stands.
 *
 * A module store in the shape of `pet/petStore.ts`, for the same reason: the
 * footer's read-out, the Settings panel and the layer that draws the windows
 * sit at different depths, and one list is one answer. **The list is the
 * node's** (`GET /addons`, re-read on `addons_changed`, on a `settings_changed`
 * that names the machine's switch, and when the bus comes back); so is the
 * machine's switch (`switchedOn`, the node's `addons_enabled`) — one source,
 * so the layer, the footer and the panel never disagree. What is this
 * machine's is furniture — whether the layer shows, which windows the person
 * put away, and each window's dock and size — and lives in localStorage under
 * documented keys (`crates/desktop.md`).
 *
 * A re-read keeps an addon's object when its record did not move
 * (`sameAddon`): a window holds the object, and a new one for the same facts
 * would reset what it holds. A switch is **optimistic**: the list reads as
 * the node will read it before the PATCH lands, and comes back if the node
 * refuses. `useAddonsSync` is mounted once, by the layer; every other reader
 * only reads.
 *
 * `snapshot()` returns the state and nothing else, as React 19 requires.
 */

import { useEffect } from "react";
import { useSyncExternalStore } from "react";
import { api } from "../api";
import { useEngineEvents, watchConnection } from "../bus";
import { errorFields, log } from "../log";
import { forgetPref, jsonPref, readPref, switchPref, switchWord, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import { createLatest } from "../shell/latestModel.mjs";
import { reloadOnReconnect } from "../shell/workspaceLoadModel.mjs";
import type { Addon, AddonPermission } from "../types";
import type { Placement } from "../ui/Dock";
import { STORAGE_KEY_PREFIX } from "./addonBridgeModel.mjs";
import { manifestWindow, pruneHidden, sameAddon, withEnabled } from "./addonsModel.mjs";
import { defaultPlacement, sizeBounds, windowPrefFrom } from "./addonWindowModel.mjs";
import type { Size, WindowPref } from "./addonWindowModel.mjs";

const LAYER_KEY = "bisa.addons.layer";
const HIDDEN_KEY = "bisa.addons.hidden";
/** Followed by the addon's id: `{dock, size}` for one window. */
const WINDOW_KEY_PREFIX = "bisa.addons.window.";

/** The machine's switch, a settings key: off, nothing draws or is served here. */
export const ADDONS_ENABLED_KEY = "addons.enabled";

export interface AddonsState {
  /** Every addon installed here, as the node lists them. */
  readonly addons: readonly Addon[];
  /** Whether `addons` has been read once, so "none" can be told from "not yet". */
  readonly loaded: boolean;
  /** Why the last read of the list refused, `null` once one answered — so a surface says the read failed rather than *none installed*, which nobody checked. */
  readonly failed: string | null;
  /** The machine's `addons.enabled` switch, as the node reports it. */
  readonly switchedOn: boolean;
  /** Whether the layer draws at all — the footer's switch and the chord. */
  readonly layerShown: boolean;
  /** The addons the person put away; they come back from the footer's popover, or when switched on again. */
  readonly hidden: readonly string[];
  /** A bump per window move or resize, so a window re-reads its preference. */
  readonly windowsVersion: number;
}

function readHidden(): string[] {
  return readPref(webStorage(), HIDDEN_KEY, (raw) => {
    const v = jsonPref(raw);
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  }, []);
}

let state: AddonsState = {
  addons: [],
  loaded: false,
  failed: null,
  switchedOn: true,
  layerShown: readPref(webStorage(), LAYER_KEY, switchPref, true),
  hidden: readHidden(),
  windowsVersion: 0,
};

const listeners = new Set<() => void>();

function set(next: AddonsState): void {
  if (next === state) return;
  state = next;
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function snapshot(): AddonsState {
  return state;
}

export function useAddons(): AddonsState {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** The node's list, with every object a window already holds kept when its record did not move. */
function merged(fresh: readonly Addon[]): readonly Addon[] {
  const same = fresh.length === state.addons.length && fresh.every((a, i) => sameAddon(a, state.addons[i]));
  if (same) return state.addons;
  return fresh.map((a) => state.addons.find((b) => sameAddon(a, b)) ?? a);
}

function writeHidden(next: readonly string[]): void {
  writePref(webStorage(), HIDDEN_KEY, next);
}

/** The reads of the list, in order (`latestModel`): a frame, a verb and a reconnect each ask, the node answers in whatever order, and only the newest asked for is applied — an older answer never puts back what a later one took out. */
const reads = createLatest();

/** Read the list again; the node owns it. */
export async function refreshAddons(signal?: AbortSignal): Promise<void> {
  const ticket = reads.begin();
  try {
    const r = await api.addons(signal);
    if (!reads.lands(ticket)) return;
    const addons = merged(r.addons);
    const hidden = pruneHidden(state.hidden, addons);
    if (hidden !== state.hidden) writeHidden(hidden);
    set({ ...state, addons, hidden, switchedOn: r.addons_enabled, loaded: true, failed: null });
  } catch (e) {
    if (signal?.aborted || !reads.lands(ticket)) return;
    log.warn("addons", "the addons could not be read", errorFields(e));
    // The list stays as it was — read, or not yet — and the refusal is said.
    set({ ...state, failed: e instanceof Error ? e.message : String(e) });
  }
}

/** Show or hide every window at once — the footer's switch and the chord. */
export function toggleAddonLayer(on?: boolean): void {
  const next = on ?? !state.layerShown;
  writePref(webStorage(), LAYER_KEY, switchWord(next));
  set({ ...state, layerShown: next });
}

/** Put one window away, or bring it back. */
export function setAddonHidden(id: string, hidden: boolean): void {
  const without = state.hidden.filter((h) => h !== id);
  const next = hidden ? [...without, id] : without;
  writeHidden(next);
  set({ ...state, hidden: next });
}

/**
 * Where a window stands and how big it is: what the person left, else where
 * the manifest opens it — staggered by `slot`, the addon's place among every
 * installed one, so two first openings never land on one pixel and a window
 * that shows or hides moves nobody else.
 */
export function windowOf(addon: Addon, slot: number, viewport: { width: number; height: number; top: number }): WindowPref {
  const w = manifestWindow(addon.manifest);
  const fallback = defaultPlacement(w.default_dock, slot);
  const raw = readPref(webStorage(), WINDOW_KEY_PREFIX + addon.id, (s) => s, null);
  return windowPrefFrom(raw, { width: w.width, height: w.height }, fallback, sizeBounds(w, viewport));
}

function writeWindow(id: string, pref: WindowPref): void {
  writePref(webStorage(), WINDOW_KEY_PREFIX + id, pref);
  set({ ...state, windowsVersion: state.windowsVersion + 1 });
}

export function moveAddon(id: string, dock: Placement, size: Size): void {
  writeWindow(id, { dock, size });
}

export function resizeAddon(id: string, size: Size, dock: Placement): void {
  writeWindow(id, { dock, size });
}

/** Every window back to where its manifest opens it. */
export function resetAddonPositions(): void {
  for (const a of state.addons) forgetPref(webStorage(), WINDOW_KEY_PREFIX + a.id);
  set({ ...state, windowsVersion: state.windowsVersion + 1 });
}

// --- the node's verbs, each followed by a re-read -----------------------

/**
 * Turn an addon on or off. The list reads as the node will before the PATCH
 * lands; a refusal puts it back and is thrown for the row to say. Turning
 * one on brings its window back if it was put away: a person who switches
 * an addon on wants to see it.
 */
export async function setAddonEnabled(id: string, enabled: boolean): Promise<void> {
  const was = state.addons.find((a) => a.id === id)?.enabled;
  set({ ...state, addons: withEnabled(state.addons, id, enabled) });
  if (enabled && state.hidden.includes(id)) setAddonHidden(id, false);
  try {
    await api.patchAddon(id, { enabled });
  } catch (e) {
    // This addon's switch alone goes back — on the list as it stands now, so a
    // re-read that landed meanwhile for another addon is not undone with it.
    if (was !== undefined) set({ ...state, addons: withEnabled(state.addons, id, was) });
    throw e;
  }
  await refreshAddons();
}

export async function setAddonGrants(id: string, granted: AddonPermission[]): Promise<void> {
  await api.patchAddon(id, { granted });
  await refreshAddons();
}

/** Remove an addon: the node's record and bundle, and this machine's memory of it — its window, its put-away mark, its store. */
export async function removeAddon(id: string): Promise<void> {
  await api.deleteAddon(id);
  forgetPref(webStorage(), WINDOW_KEY_PREFIX + id);
  forgetPref(webStorage(), STORAGE_KEY_PREFIX + id);
  if (state.hidden.includes(id)) setAddonHidden(id, false);
  await refreshAddons();
}

export async function importAddon(path: string, granted: AddonPermission[], enabled: boolean): Promise<Addon> {
  const r = await api.importAddon(path, granted, enabled);
  await refreshAddons();
  return r.addon;
}

export async function installAddonFromCatalog(slug: string): Promise<void> {
  await api.installCatalogEntry("addon", slug);
  await refreshAddons();
}

/**
 * Keep the list current: read on mount, again on every `addons_changed`
 * frame — the node says which addon moved, and the whole list is cheap —
 * on a `settings_changed` that names the machine's switch, and again when
 * the bus comes back after a gap. Mounted once, by the layer.
 */
export function useAddonsSync(): void {
  useEffect(() => {
    const ctrl = new AbortController();
    void refreshAddons(ctrl.signal);
    return () => ctrl.abort();
  }, []);
  useEngineEvents((e) => {
    if (e.payload.type === "addons_changed") void refreshAddons();
    if (e.payload.type === "settings_changed" && e.payload.keys.includes(ADDONS_ENABLED_KEY)) void refreshAddons();
  });
  useEffect(() => reloadOnReconnect(watchConnection, () => void refreshAddons()), []);
}

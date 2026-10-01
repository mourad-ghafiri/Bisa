/**
 * Which pet is showing and where it sits.
 *
 * A module store in the shape of `notes/notesStore.ts`, and for the same
 * reason it gives: the things that touch a pet sit at different depths, and
 * threading a provider to all of them would put "which one, and where" in the
 * hands of whoever wires the next one.
 *
 * **`snapshot()` returns the state and nothing else** — React 19 throws *"The
 * result of getSnapshot should be cached to avoid an infinite loop"* the
 * moment it returns a freshly-built object.
 *
 * `active` and `dock` persist, because both are preferences about this
 * machine's window; `lastActive` does not — a relaunch with the pet put away
 * shows the default (Moonrice) when it is turned on again. Which pet you chose is not workspace truth — the packages
 * are, and they live in the node; this is taste, and it belongs beside the
 * theme.
 *
 * **`pets` lives here too, and that is the fix for a bug.** The overlay and the
 * settings panel each used to fetch their own copy, and nothing told the
 * overlay when the panel installed one — so a freshly imported pet was
 * installed, selected, and invisible until the app restarted. Two copies of one
 * list is two answers to one question; there is one list now, and both read it.
 */

import { useSyncExternalStore } from "react";
import { api } from "../api";
import { errorFields, log } from "../log";
import { forgetPref, jsonPref, readPref, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import type { PetDef } from "../types";
import { placementFrom, samePlacement, type Placement } from "../ui/Dock";
import { PET_SIZE_DEFAULT, clampPetSize, defaultPet } from "./petModel.mjs";

const ACTIVE_KEY = "bisa.pet.active";
const DOCK_KEY = "bisa.pet.dock";
const SIZE_KEY = "bisa.pet.size";

/**
 * Where the pet starts, before anyone has dragged it.
 *
 * Clear of the notes dock, which defaults to 24 from the bottom right and is
 * `DOCK_SIZE` wide — two draggable things landing on the same pixel on first
 * run would read as one broken thing rather than two features.
 */
const DEFAULT_DOCK: Placement = { h: "right", x: 96, v: "bottom", y: 24 };

export interface PetState {
  /** The pet's id, or null when none is chosen or installed. */
  active: string | null;
  /** The last pet shown this session, so *Show the pet* brings the same one back. */
  lastActive: string | null;
  /** Anchored to its nearest edges, so it keeps its place when the window changes size. */
  dock: Placement;
  /** How big it stands, as a percentage of the default height. */
  size: number;
  /** Every installed package. Not persisted: the node owns these. */
  pets: PetDef[];
  /** Whether `pets` has been read once, so "none" can be told from "not yet". */
  loaded: boolean;
}

const activeAtStart = readPref(webStorage(), ACTIVE_KEY, (raw) => raw || null, null);

let state: PetState = {
  pets: [],
  loaded: false,
  active: activeAtStart,
  lastActive: activeAtStart,
  // Clamped on the way *out* of storage as well as in: the bounds can tighten
  // in a later version, and a number written by the looser one must not be
  // able to draw a pet the current rules would refuse.
  size: readPref(webStorage(), SIZE_KEY, clampPetSize, PET_SIZE_DEFAULT),
  // The model is the one judge of what a placement is; an earlier shape or
  // a corrupt value takes the default.
  dock: readPref(webStorage(), DOCK_KEY, (raw) => placementFrom(jsonPref(raw), DEFAULT_DOCK), DEFAULT_DOCK),
};

const listeners = new Set<() => void>();

function set(next: PetState) {
  if (next === state) return;
  state = next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function snapshot(): PetState {
  return state;
}

export function usePet(): PetState {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** Choose a pet, or `null` to put it away — the one put away is remembered. */
export function setActivePet(id: string | null): void {
  if (state.active === id) return;
  writePref(webStorage(), ACTIVE_KEY, id || null);
  set({ ...state, active: id, lastActive: id ?? state.lastActive });
}

/**
 * Show a pet when none is chosen — the last one shown, else the default
 * (`petModel.defaultPet`: Moonrice while it ships) — and put it away when one
 * is. The footer's switch and the panel's speak this one verb.
 */
export function togglePet(on: boolean): void {
  if (!on) {
    setActivePet(null);
    return;
  }
  setActivePet(defaultPet(state.pets, state.lastActive));
}

/**
 * Resize the pet, within the bounds the model sets.
 *
 * The placement is left alone. It is measured from the edges the pet is
 * nearest, so a pet that grows does so away from them — and the paint is
 * re-clamped against the window on every render anyway.
 */
export function setPetSize(size: number): void {
  const next = clampPetSize(size);
  if (next === state.size) return;
  writePref(webStorage(), SIZE_KEY, String(next));
  set({ ...state, size: next });
}

/**
 * Move the pet to a placement the drag made.
 *
 * It arrives clamped and anchored by the model (`placementOf`, from the box
 * the pointer moved), so a placement that could strand it off-screen is never
 * the one that gets stored.
 */
export function movePet(next: Placement): void {
  if (samePlacement(next, state.dock)) return;
  writePref(webStorage(), DOCK_KEY, next);
  set({ ...state, dock: next });
}

/** Whether the pet has been dragged away from where it starts. */
export function petMoved(dock: Placement): boolean {
  return !samePlacement(dock, DEFAULT_DOCK);
}

/**
 * Put the pet back in its corner.
 *
 * Anchored to its nearest edges, a pet parked top-left stays top-left for
 * good — so the way back to where it started has to exist, as the notes
 * dock's does.
 */
export function resetPetPosition(): void {
  forgetPref(webStorage(), DOCK_KEY);
  set({ ...state, dock: DEFAULT_DOCK });
}

/**
 * Re-read the installed packages.
 *
 * Called on mount by whoever needs the list, and after every install or
 * removal. Failure is silent: no pets is the ordinary state on a fresh
 * workspace, and an unreachable node already says so in the chrome — but
 * `loaded` is set either way, so a caller can tell "none" from "not yet".
 */
export async function refreshPets(signal?: AbortSignal): Promise<void> {
  try {
    const { pets } = await api.pets(signal);
    if (signal?.aborted) return;
    set({ ...state, pets, loaded: true });
  } catch (e) {
    if (signal?.aborted) return;
    // No toast — but never nothing: the log says why no pet is listed.
    log.warn("pet", "the installed pets could not be read", errorFields(e));
    set({ ...state, loaded: true });
  }
}

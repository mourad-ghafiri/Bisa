/**
 * Where a person was, kept (`placeMemoryModel.mjs` has the rules): the last
 * place, each section's, the query each path was left with — window
 * furniture, in `localStorage` under `bisa.view.places`, read once when this
 * module loads so the router may ask before anything mounts.
 *
 * The router is the one writer: every hash it stands on goes through
 * `land`. An arrival is written at once — the place itself must survive a
 * quit a second later — and a change in place waits a beat, since a filter
 * box writes an entry a keystroke. Storage that cannot be read or written
 * costs the memory, never the navigation (`storedPrefModel.mjs`).
 */

import { useSyncExternalStore } from "react";
import {
  PLACES_KEY,
  adoptPlaces,
  arrive,
  emptyPlaces,
  forgetPath,
  launchHash,
  parsePlaces,
  pathOf,
  type Places,
} from "./placeMemoryModel.mjs";
import { jsonPref, readPref, webStorage, writePref } from "./storedPrefModel.mjs";

/** How long a change in place waits for the next one before it is written. */
const IN_PLACE_BEAT_MS = 250;

let places: Places = readPref(webStorage(), PLACES_KEY, (raw) => parsePlaces(jsonPref(raw)), emptyPlaces());
const listeners = new Set<() => void>();
let timer: number | null = null;
/** When the storage last took a write; 0 before any. */
let writtenAt = 0;
/** The path last landed on, and whether the memory had been on it before. */
let arrived: { path: string; known: boolean } = { path: "", known: false };
/** Whether the memory takes anything more: not once *Forget where I was* sealed it, until the window opens again. */
let sealed = false;

function write(): void {
  if (timer !== null) window.clearTimeout(timer);
  timer = null;
  if (writePref(webStorage(), PLACES_KEY, places)) writtenAt = Date.now();
}

function set(next: Places, at: "once" | "beat"): void {
  if (sealed || next === places) return;
  places = next;
  if (at === "once") write();
  else {
    if (timer !== null) window.clearTimeout(timer);
    timer = window.setTimeout(write, IN_PLACE_BEAT_MS);
  }
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/**
 * A hash lands: the hash to stand on, resolved against the memory, and the
 * landing recorded. `exact` is the router's word that the entry is to be
 * taken as it is.
 */
export function land(hash: string, exact: boolean): string {
  const path = pathOf(hash);
  const out = arrive(places, hash, { exact });
  // A change in place keeps what the arrival learned: the screen asks about the place, not the entry.
  if (path !== arrived.path) arrived = { path, known: out.known };
  set(out.places, exact ? "beat" : "once");
  return out.land;
}

/** Where a launch with no hash opens, or `null` — the home. */
export function launchPlace(): string | null {
  return launchHash(places);
}

/**
 * Whether the memory had been on `path` before the person arrived on it —
 * what a screen asks to tell a first visit from a return. False for a path
 * that is not the one stood on.
 */
export function placeWasKnown(path: string): boolean {
  return arrived.path === path && arrived.known;
}

/** A thing is gone: its path, and what is under it, is forgotten. */
export function forgetPlace(path: string): void {
  set(forgetPath(places, path), "once");
}

/** The workspace the memory is for; another's is forgotten whole. */
export function adoptPlaceOwner(owner: string): void {
  set(adoptPlaces(places, owner), "once");
}

/** Forget every place — *Forget where I was*. The owner stays. */
export function forgetEveryPlace(): void {
  arrived = { path: "", known: false };
  set(emptyPlaces(places.owner), "once");
}

/** Write what is waiting, now — the way out, the window hiding. */
export function flushPlaces(): void {
  if (timer !== null && !sealed) write();
}

/**
 * Take no place more and write none, for the rest of this window: a hash
 * still lands — the navigation is never the memory's to refuse — and is
 * recorded by the window that opens next.
 */
export function sealPlaces(): void {
  flushPlaces();
  if (timer !== null) window.clearTimeout(timer);
  timer = null;
  sealed = true;
}

/** When the storage last took a write of the places; 0 before any. */
export function placesWrittenAt(): number {
  return writtenAt;
}

/** The memory as it stands, for a reader with no render to wait for. */
export function placesNow(): Places {
  return places;
}

/** The memory, as a subscription — what a section's door reads. */
export function usePlaces(): Places {
  return useSyncExternalStore(subscribe, () => places, () => places);
}

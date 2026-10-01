/**
 * Which conversation each owner was last on (13 — Conversations): the IDE's
 * roots, the Workflow Designer's workflows, the notes and the drawings, one
 * memory for all — so a door back to any of them lands on the conversation
 * the person left, and a hand-off from the editor knows where the checkout
 * stands. Window furniture, in `localStorage` under `bisa.conversations.pick`,
 * per owner (`ownerKey`: `workstream:<wid>`, `workflow:<id>`, `note:<id>`…),
 * the newest owners kept. The rules are `conversationSurfaceModel.mjs`'s
 * (`parsePicks`, `rememberPick`); this file only keeps. Storage that cannot be
 * read or written costs the memory, never the surface (`storedPrefModel.mjs`).
 */

import { useCallback, useSyncExternalStore } from "react";
import { jsonPref, readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { PICK_KEY, parsePicks, rememberPick } from "./conversationSurfaceModel.mjs";

type Picks = Readonly<Record<string, string>>;

function read(): Picks {
  return readPref(webStorage(), PICK_KEY, (raw) => parsePicks(jsonPref(raw)), {});
}

let picks: Picks = read();
const listeners = new Set<() => void>();

function set(next: Picks): void {
  if (next === picks) return;
  picks = next;
  writePref(webStorage(), PICK_KEY, next);
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The owner's remembered pick, read once — what a hand-off reads before it decides where to land. */
export function rememberedPick(owner: string): string | null {
  return picks[owner] ?? null;
}

/** Remember `id` as the owner's pick — the newest — or forget the owner's (`null`). */
export function keepPick(owner: string, id: string | null): void {
  set(rememberPick(picks, owner, id));
}

/**
 * The owner's remembered pick, as a subscription, and its setter — stable for
 * the owner, so an effect may depend on it.
 */
export function useRememberedPick(owner: string): [string | null, (id: string | null) => void] {
  const kept = useSyncExternalStore(
    subscribe,
    () => picks[owner] ?? null,
    () => picks[owner] ?? null,
  );
  const keep = useCallback((id: string | null) => keepPick(owner, id), [owner]);
  return [kept, keep];
}

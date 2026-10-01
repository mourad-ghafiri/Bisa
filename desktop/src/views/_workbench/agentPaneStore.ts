/**
 * What is attached to the next message in the agent pane (ide/09), **per
 * conversation scope** — `workstream:<id>`, and `<kind>:<id>` for a goal's,
 * a channel's or a message's own composer (`chatScopeModel.chatKey`, ide/18)
 * — because a chip is a fact about one thread: a hunk attached while looking at one workstream must not turn
 * up on the message sent from another. One module-level store, because the
 * things that attach a chip (the explorer, a diff, the editor's selection
 * binding, a terminal tab) live at different depths under different screens;
 * they name the scope, or leave it to the route — the workbench the person is
 * looking at is the thread they are attaching to.
 *
 * Chips survive a navigation and a restart with the draft they belong to:
 * `bisa:context:<scope>` in `localStorage`, beside the composer's
 * `bisa:draft:<scope>`. Whether the pane *shows* is the right panel's
 * (`rightPanelStore.ts`): attaching a chip opens its Agent tab.
 */

import { useSyncExternalStore } from "react";
import { currentRoute } from "../../router";
import { jsonPref, readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import type { ContextRef } from "../../types";
import { attach as attachChip, sameChip } from "./contextChips.mjs";
import { rootKey } from "./workbenchModel.mjs";
import { showRightPanel } from "./rightPanelStore";

const EMPTY: readonly ContextRef[] = Object.freeze([]);
const storageKey = (scope: string) => `bisa:context:${scope}`;

let byScope: Readonly<Record<string, readonly ContextRef[]>> = {};
/** Scopes whose stored chips have been read once. */
const loaded = new Map<string, readonly ContextRef[]>();
const listeners = new Set<() => void>();

function read(scope: string): readonly ContextRef[] {
  const v = readPref(webStorage(), storageKey(scope), jsonPref, null);
  return Array.isArray(v) ? (v as ContextRef[]) : EMPTY;
}

function write(scope: string, refs: readonly ContextRef[]): void {
  writePref(webStorage(), storageKey(scope), refs.length > 0 ? refs : null);
}

/**
 * A scope's stored chips, read once and remembered — what a scope shows
 * before anything is attached this session. A read, so a render never
 * moves the store's state; the first write (`set`) is what moves it.
 */
function stored(scope: string): readonly ContextRef[] {
  const known = loaded.get(scope);
  if (known) return known;
  const fresh = read(scope);
  const kept = fresh.length > 0 ? fresh : EMPTY;
  loaded.set(scope, kept);
  return kept;
}

/** What a scope holds: what was set this session, else what is stored. */
function current(scope: string): readonly ContextRef[] {
  return byScope[scope] ?? stored(scope);
}

function set(scope: string, next: readonly ContextRef[]): void {
  if (byScope[scope] === next) return;
  byScope = { ...byScope, [scope]: next };
  write(scope, next);
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The scope the route's workbench is rooted at — the thread a chip attaches to by default. */
function contextScope(): string | null {
  const r = currentRoute();
  return r.name === "workbench" ? rootKey(r.scope, r.id) : null;
}

/** The chips for one scope. */
export function contextFor(scope: string): readonly ContextRef[] {
  return current(scope);
}

export function useAgentPane(scope: string): { context: readonly ContextRef[] } {
  const context = useSyncExternalStore(
    subscribe,
    () => current(scope),
    () => EMPTY,
  );
  return { context };
}

/**
 * Add a chip (replacing an earlier chip for the same thing) and show the
 * pane. Without `scope`, the route's workbench; away from any workbench the
 * chip has no thread to join and only the pane opens.
 */
export function attachContext(ref: ContextRef, scope: string | null = contextScope()): void {
  if (scope) set(scope, attachChip([...contextFor(scope)], ref));
  showRightPanel("agents", scope ?? undefined);
}

/**
 * Add a chip to a thread's tray and show nothing: the door a tray outside
 * the IDE uses — the Browser pane's annotations joining the conversation on
 * screen (ide/18), whose composer is already in view.
 */
export function attachTo(scope: string, ref: ContextRef): void {
  set(scope, attachChip([...contextFor(scope)], ref));
}

/**
 * Replace the tray with `next` — the bulk door (`gitContext.ts`), which
 * attaches many chips under one budget check and shows the pane itself.
 */
export function setContext(scope: string, next: readonly ContextRef[]): void {
  set(scope, next);
}

export function removeContext(scope: string, index: number): void {
  set(
    scope,
    contextFor(scope).filter((_, i) => i !== index),
  );
}

/** Drop the chip that is the same thing as `ref`, if it is there. */
export function removeContextRef(scope: string, ref: ContextRef): void {
  const cur = contextFor(scope);
  const next = cur.filter((r) => !sameChip(r, ref));
  if (next.length !== cur.length) set(scope, next);
}

export function clearContext(scope: string): void {
  if (contextFor(scope).length === 0) return;
  set(scope, EMPTY);
}

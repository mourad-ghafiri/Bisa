/**
 * The order of the sidebar's destinations — the person's, kept per viewer in
 * `localStorage` under one key, the way the footer keeps its dimension
 * (`resourceDimensionStore.ts`). Read once at start and made whole against
 * `PRIMARY_NAV` by `navOrderModel.orderKeys`; every surface that draws the
 * destinations — the sidebar, its rail, the palette's *Go to* — reads
 * `usePrimaryNav()` here, so a drag in one is the order in all — and the
 * router's home is the order's first (`homeRoute`), so the app opens where
 * the person put the first row.
 */

import { useSyncExternalStore } from "react";
import { PRIMARY_NAV, type NavEntry } from "./nav";
import { NAV_ORDER_KEY, homeKey, isDefaultOrder, orderKeys, orderedNav, placeKey } from "./navOrderModel.mjs";
import { forgetPref, jsonPref, readPref, webStorage, writePref } from "./storedPrefModel.mjs";

const DEFAULTS: readonly string[] = PRIMARY_NAV.map((e) => e.key);

let order: readonly string[] = orderKeys(readPref(webStorage(), NAV_ORDER_KEY, jsonPref, null), DEFAULTS);
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function set(next: readonly string[]): void {
  if (next === order) return;
  order = next;
  if (isDefaultOrder(order, DEFAULTS)) forgetPref(webStorage(), NAV_ORDER_KEY);
  else writePref(webStorage(), NAV_ORDER_KEY, order);
  for (const l of listeners) l();
}

/** The destinations' keys in the person's order, as a subscription. */
function useNavOrder(): readonly string[] {
  return useSyncExternalStore(subscribe, () => order, () => DEFAULTS);
}

/** The destinations in the person's order — what every surface draws. */
export function usePrimaryNav(): readonly NavEntry[] {
  const keys = useNavOrder();
  return orderedNav(PRIMARY_NAV, keys);
}

/**
 * Where the app opens — the first destination of the person's order — as a
 * plain read: the order is loaded when this module is, so the router may ask
 * before anything mounts.
 */
export function homeRoute(): NavEntry["route"] {
  const key = homeKey(order, DEFAULTS);
  return (PRIMARY_NAV.find((e) => e.key === key) ?? PRIMARY_NAV[0]).route;
}

/** Put a destination at `index`: a drag landed, or the keyboard dropped it. */
export function placeNav(key: string, index: number): void {
  set(placeKey(order, key, index));
}

/** Back to the file's order. */
export function resetNavOrder(): void {
  set(DEFAULTS);
}

/** Whether the person's order is the file's, as a subscription. */
export function useNavOrderIsDefault(): boolean {
  const keys = useNavOrder();
  return isDefaultOrder(keys, DEFAULTS);
}

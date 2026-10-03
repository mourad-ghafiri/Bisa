/**
 * The devices this machine can reach (ide/19), read from the node once and
 * again on every `mobile_development_changed` frame — one list every surface shares:
 * the Devices button, a device document's strip tab and bar, Settings. The
 * node lists them (`GET /mobile-development/devices`) and filters by the platforms that
 * are on; this store only keeps the answer. A read that the node refuses —
 * mobile development off — is an error here and nothing more: the surfaces
 * that read it are hidden while it is off.
 */

import { useSyncExternalStore } from "react";
import { api } from "../api";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import type { MobileDevice } from "../types";
import { failureReason } from "../ui/failure";

export interface DevicesState {
  readonly devices: readonly MobileDevice[];
  /** The node has answered once — with a list or a refusal. */
  readonly read: boolean;
  readonly error: string | null;
}

let state: DevicesState = { devices: [], read: false, error: null };
const listeners = new Set<() => void>();
let inflight: Promise<void> | null = null;

function set(next: DevicesState): void {
  state = next;
  for (const l of listeners) l();
}

/** Read the list again — single-flight, so a burst of frames is one request. */
export function refreshDevices(): Promise<void> {
  if (inflight) return inflight;
  inflight = api
    .mobileDevelopmentDevices()
    .then(
      (r) => set({ devices: r.devices, read: true, error: null }),
      (e: unknown) => set({ ...state, read: true, error: failureReason("devices", "the devices could not be listed", e) }), // for the log
    )
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/**
 * The devices, as a subscription. `load` says whether this surface wants
 * the list read at all — a strip with no device tab asks the node nothing.
 */
export function useDevices(load = true): DevicesState {
  const snapshot = useSyncExternalStore(subscribe, () => state, () => state);
  if (load && !snapshot.read && !inflight) void refreshDevices();
  // What a restarted node forgot, and what moved while it was away, reaches
  // the list by no frame: every surface that reads the devices reads again.
  useReloadOnReconnect(() => {
    if (load) void refreshDevices();
  });
  return snapshot;
}

/**
 * What the shell says of its node, kept for every reader (`nodeBootModel`):
 * the boot's phase while it boots, the failure while there is one, ready
 * once it answers. Installed once from `main.tsx`, above the root boundary,
 * so the root crash card reads it like the sidebar's footer does. The first
 * state comes from the shell's own word (`node_status`), since the events
 * may have been said before the webview listened; every event after that
 * folds in as it comes. A restart drops the cached API base, so a port that
 * moved is never dialled from memory.
 */

import { useSyncExternalStore } from "react";
import { forgetApiBase, inDesktopShell } from "../api";
import { errorFields, log } from "../log";
import { nodeStatus } from "./nodeApi";
import { NODE_BOOT_INITIAL, NODE_EVENTS, reduceNodeEvent, seedFromStatus, type NodeBootState } from "./nodeBootModel.mjs";

let state: NodeBootState = NODE_BOOT_INITIAL;
const listeners = new Set<() => void>();

function set(next: NodeBootState): void {
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

/** The node's boot state, as a subscription. */
export function useNodeBoot(): NodeBootState {
  return useSyncExternalStore(subscribe, () => state, () => state);
}

let installed = false;

/** Listen to the shell's three node events, once; nothing off the desktop shell. */
export function installNodeBootStore(): void {
  if (installed || !inDesktopShell()) return;
  installed = true;
  void nodeStatus().then(
    (status) => set(seedFromStatus(state, status)),
    (e: unknown) => log.debug("shell", "the node's status could not be read at boot", errorFields(e)),
  );
  void import("@tauri-apps/api/event").then((ev) => {
    void ev.listen<unknown>(NODE_EVENTS.boot, (e) => set(reduceNodeEvent(state, NODE_EVENTS.boot, e.payload)));
    void ev.listen<Record<string, unknown>>(NODE_EVENTS.failed, (e) => {
      set(reduceNodeEvent(state, NODE_EVENTS.failed, e.payload));
      log.warn("shell", "the node is not running", { kind: e.payload.kind, attempt: e.payload.attempt, next_in_secs: e.payload.next_in_secs });
    });
    // The sidecar restarted the node — by its watchdog after a crash, or
    // because a person asked. The base is resolved again on the next call;
    // the lists are read again by the shell when the bus comes back
    // (`useWorkspaceData`).
    void ev.listen<{ port: number; attempt: number; requested: boolean }>(NODE_EVENTS.restarted, (e) => {
      forgetApiBase();
      set(reduceNodeEvent(state, NODE_EVENTS.restarted, e.payload));
      log.warn("shell", e.payload.requested ? "the node was restarted" : "the node restarted after exiting on its own", { port: e.payload.port, attempt: e.payload.attempt });
    });
  });
}

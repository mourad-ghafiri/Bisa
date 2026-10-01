/**
 * Hold the watcher's lease on a root while something shows it:
 * the editor for its document, the explorer for its tree, the Git tab for
 * its checkout. Re-posted before the node's idle window closes, and again
 * the moment the bus comes back after the node was away — a restarted node
 * has no watches, and four minutes of silence would be four minutes of
 * stale panels. A root with no directory yet is not an error — the
 * engine's own frames still say when files changed.
 */
import { useEffect } from "react";
import { api } from "../api";
import { watchConnection } from "../bus";
import { errorFields, log } from "../log";
import { reloadOnReconnect } from "../shell/workspaceLoadModel.mjs";
import type { FileScope } from "../types";

/** Re-post well inside the node's five-minute idle window. */
const WATCH_LEASE_MS = 4 * 60 * 1000;

export function useWatchLease(scope: FileScope, id: string, enabled = true): void {
  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    const lease = () => {
      api.ideWatch(scope, id).catch((e: unknown) => {
        // Nothing to watch yet; the frames that mean files changed still arrive.
        log.debug("watch", "the watch lease was refused; the frames that mean files changed still arrive", { scope, id, ...errorFields(e) });
      });
    };
    lease();
    const t = window.setInterval(() => alive && lease(), WATCH_LEASE_MS);
    const unwatch = reloadOnReconnect(watchConnection, () => alive && lease());
    return () => {
      alive = false;
      window.clearInterval(t);
      unwatch();
    };
  }, [scope, id, enabled]);
}

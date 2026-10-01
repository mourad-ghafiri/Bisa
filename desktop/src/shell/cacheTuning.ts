/**
 * Applies the desktop's `cache.*` settings — and the network echo URL — to
 * the module stores.
 *
 * The Rust caches read their TTLs from the node's settings directly; the
 * desktop's own knobs — the path-index LRU size and the poll cadences — live in
 * module stores that are not React, so this reads the resolved settings once and
 * pushes them into those stores. Called at startup and again on `settings_changed`
 * from `App`, exactly like `syncAppearanceFromNode` (`theme.ts`).
 */

import { api } from "../api";
import { numberOf, stringOf } from "./settingsModel.mjs";
import { setMaxRoots } from "./pathIndexStore";
import { setPortsPollMs } from "./portsStore";
import { setStatsPollMs } from "./statsStore";
import { setNetworkPollMs, setPublicIpUrl } from "./networkStore";
import { setSessionsSafetyMs } from "./sessionsStore";

/** Read the resolved `cache.*` settings and apply them to the stores. */
export async function syncCacheTuningFromNode(): Promise<void> {
  try {
    const { settings } = await api.settingsResolved(null);
    setMaxRoots(numberOf(settings, "cache.path_index.max_roots", 8, { min: 1, max: 128 }));
    setPortsPollMs(numberOf(settings, "cache.desktop.ports_poll_ms", 5_000, { min: 500 }));
    setStatsPollMs(
      numberOf(settings, "cache.desktop.stats_poll_ms", 5_000, { min: 500 }),
      numberOf(settings, "cache.desktop.disk_poll_ms", 60_000, { min: 5_000 }),
    );
    setNetworkPollMs(numberOf(settings, "cache.desktop.network_poll_ms", 30_000, { min: 5_000 }));
    setPublicIpUrl(stringOf(settings, "network.public_ip_url", "https://api.ipify.org"));
    setSessionsSafetyMs(numberOf(settings, "cache.desktop.sessions_safety_ms", 60_000, { min: 5_000 }));
  } catch {
    // No node yet, or a read that failed: the built-in defaults stand.
  }
}

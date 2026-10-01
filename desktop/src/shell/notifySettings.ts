/**
 * The notification switches, followed (Settings › System › Notifications):
 * read from the resolved machine layer once at boot, again on every
 * `settings_changed` that names one and when the bus comes back after the
 * node was away, held in a module variable the door
 * reads synchronously — a notice is decided the moment its frame lands. The
 * same shape as `closeGuardSettings.ts`: the registry's defaults stand until
 * the node answers.
 */

import { useEffect } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { NOTIFY_DEFAULTS, namesNotifyKey, readNotifyPrefs, type NotifyPrefs } from "./notificationsModel.mjs";

let prefs: NotifyPrefs = { ...NOTIFY_DEFAULTS };

/** What the switches say right now. */
export function notifyPrefs(): NotifyPrefs {
  return prefs;
}

async function load(): Promise<void> {
  try {
    const { settings } = await api.settingsResolved(null);
    prefs = readNotifyPrefs(settings);
  } catch {
    // No node yet: the defaults stand until there is one.
  }
}

/** Mounted once, by `App`. */
export function useNotifySettings(): void {
  useEffect(() => {
    void load();
  }, []);
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && namesNotifyKey(e.payload.keys)) void load();
  });
  // A node that was not there at boot left the defaults standing: read once it is.
  useReloadOnReconnect(() => void load());
}

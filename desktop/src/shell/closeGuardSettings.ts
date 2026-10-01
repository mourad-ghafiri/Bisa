/**
 * The three confirmation switches, followed: read from the resolved machine
 * layer once at boot and again on every `settings_changed` that names one,
 * held in a module variable the close guards read synchronously — a close
 * request cannot wait on a round trip. The same shape as `logSettings.ts`:
 * the registry's defaults stand until the node answers.
 */

import { useEffect } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { CONFIRM_DEFAULTS, namesConfirmKey, readConfirmPrefs, type ConfirmPrefs } from "./closeGuardModel.mjs";

let prefs: ConfirmPrefs = { ...CONFIRM_DEFAULTS };

/** What the switches say right now. */
export function confirmPrefs(): ConfirmPrefs {
  return prefs;
}

async function load(): Promise<void> {
  try {
    const { settings } = await api.settingsResolved(null);
    prefs = readConfirmPrefs(settings);
  } catch {
    // No node yet: every confirmation on, the default, until there is one.
  }
}

/** Mounted once, by `App`. */
export function useCloseGuardSettings(): void {
  useEffect(() => {
    void load();
  }, []);
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && namesConfirmKey(e.payload.keys)) void load();
  });
  // A node that was not there at boot left the defaults standing: read once it is.
  useReloadOnReconnect(() => void load());
}

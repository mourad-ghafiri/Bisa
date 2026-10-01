/**
 * The `logging.*` settings, followed: read from the resolved machine layer
 * once at boot and again on every `settings_changed` that names one, then
 * handed to the logger — the webview's gate and, through it, the shell's
 * file layer (`log.ts`). The same shape as `artifactSettings.ts` and
 * `cacheTuning.ts`: the registry's default stands until the node answers.
 */

import { useEffect } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { configureLog } from "../log";
import { configFrom } from "../logModel.mjs";

async function load(): Promise<void> {
  try {
    const { settings } = await api.settingsResolved(null);
    configureLog(configFrom(settings));
  } catch {
    // No node yet: errors only, the default, until there is one.
  }
}

/** Mounted once, by `App`. */
export function useLogSettings(): void {
  useEffect(() => {
    void load();
  }, []);
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && e.payload.keys.some((k) => k.startsWith("logging."))) {
      void load();
    }
  });
  // A node that was not there at boot left the default standing: read once it is.
  useReloadOnReconnect(() => void load());
}

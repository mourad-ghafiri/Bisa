/**
 * The two menu bar switches, followed (`trayModel.TRAY_KEYS`): read from the
 * resolved machine layer once at boot, again on every `settings_changed`
 * that names one and when the bus comes back after the node was away, held
 * in a module variable the close guard reads
 * synchronously — a held close request cannot wait on a round trip. The
 * same shape as `closeGuardSettings.ts`: the registry's defaults stand until
 * the node answers.
 *
 * The Dock is the setting's to rule: after every load the shell is told
 * (`tray_dock`), so a switch flipped here, in the icon's menu (which the
 * webview records, `useTray.ts`) or from the CLI ends the same way. The app
 * launches with a Dock icon and loses it a moment later when the switch is
 * off — the shell cannot read a setting before the webview does.
 */

import { useEffect } from "react";
import { api, inDesktopShell } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { errorFields, log } from "../log";
import { TRAY_DEFAULTS, namesTrayKey, readTrayPrefs } from "./trayModel.mjs";
import type { TrayPrefs } from "./trayModel.mjs";

let prefs: TrayPrefs = { ...TRAY_DEFAULTS };

/** What the switches say right now. */
export function trayPrefs(): TrayPrefs {
  return prefs;
}

async function applyDock(): Promise<void> {
  if (!inDesktopShell()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const answer = await invoke<{ visible: boolean; supported: boolean; detail?: string }>("tray_dock", { visible: prefs.dockIcon });
    if (!answer.supported) log.debug("tray", "this platform has no Dock to show or hide", { detail: answer.detail });
  } catch (e) {
    log.warn("tray", "the Dock switch did not reach the shell", errorFields(e));
  }
}

async function load(): Promise<void> {
  try {
    const { settings } = await api.settingsResolved(null);
    prefs = readTrayPrefs(settings);
  } catch {
    // No node yet: the defaults — hide on close, show in the Dock — until there is one.
  }
  await applyDock();
}

/** Mounted once, by `App`. */
export function useTrayPrefs(): void {
  useEffect(() => {
    void load();
  }, []);
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && namesTrayKey(e.payload.keys)) void load();
  });
  // A node that was not there at boot left the defaults standing: read once it is.
  useReloadOnReconnect(() => void load());
}

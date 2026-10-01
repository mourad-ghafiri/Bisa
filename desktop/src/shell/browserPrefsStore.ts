/**
 * The browser's own settings as the shell reads them (ide/18): whether this
 * machine shows a browser at all, the home page, whether tabs are
 * remembered, who may drive it and when a tab is kept out of sight, and how
 * wide a screenshot is. One module store, fed from the resolved settings by
 * `BrowserPanel` (the one always-mounted browser component) so the tab
 * store, the bridge and every launcher read one answer outside React.
 */

import { useSyncExternalStore } from "react";
import type { ResolvedSetting } from "../types";
import { AGENTS_KEY, DEFAULT_HEADLESS, DEFAULT_POLICY, ENABLED_KEY, HEADLESS_KEY, HOME_KEY, REACH_KEY, REMEMBER_KEY } from "../views/_settings/browserSettingsModel.mjs";
import { DEFAULT_SHOT_WIDTH, SHOT_WIDTH_KEY, shotWidth } from "./browserShotModel.mjs";
import { boolOf, stringOf } from "./settingsModel.mjs";

export interface BrowserPrefs {
  /** The settings have been read once; before that the defaults stand. */
  readonly read: boolean;
  readonly enabled: boolean;
  readonly home: string;
  readonly remember: boolean;
  readonly shotWidth: number;
  readonly agents: string;
  readonly reach: string;
  /** When an agent's tab is kept out of sight — the engine decides per request; the panel's words read it here. */
  readonly headless: string;
}

const DEFAULTS: BrowserPrefs = { read: false, enabled: true, home: "", remember: true, shotWidth: DEFAULT_SHOT_WIDTH, agents: DEFAULT_POLICY, reach: "anywhere", headless: DEFAULT_HEADLESS };

let prefs: BrowserPrefs = DEFAULTS;
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The prefs from the resolved settings — the workspace's, since the browser is the window's. */
export function prefsOf(resolved: readonly ResolvedSetting[] | null): BrowserPrefs {
  if (!resolved) return DEFAULTS;
  return {
    read: true,
    enabled: boolOf(resolved, ENABLED_KEY, true),
    home: stringOf(resolved, HOME_KEY, "").trim(),
    remember: boolOf(resolved, REMEMBER_KEY, true),
    shotWidth: shotWidth(resolved.find((r) => r.key === SHOT_WIDTH_KEY)?.value),
    agents: stringOf(resolved, AGENTS_KEY, DEFAULT_POLICY),
    reach: stringOf(resolved, REACH_KEY, "anywhere"),
    headless: stringOf(resolved, HEADLESS_KEY, DEFAULT_HEADLESS),
  };
}

export function setBrowserPrefs(next: BrowserPrefs): void {
  const same = (Object.keys(next) as (keyof BrowserPrefs)[]).every((k) => next[k] === prefs[k]);
  if (same) return;
  prefs = next;
  for (const l of listeners) l();
}

export function browserPrefs(): BrowserPrefs {
  return prefs;
}

export function useBrowserPrefs(): BrowserPrefs {
  return useSyncExternalStore(subscribe, () => prefs, () => prefs);
}

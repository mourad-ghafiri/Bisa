/**
 * The canvas's own settings as the desktop reads them (19 — Drawings):
 * whether this machine draws at all, who may draw, how wide a snapshot is.
 * One module store, fed from the resolved settings by `DrawPanel` (the one
 * always-mounted Draw component), so the bridge reads one answer outside
 * React — the shape of `shell/browserPrefsStore.ts`.
 */

import { useSyncExternalStore } from "react";
import type { ResolvedSetting } from "../types";
import { boolOf, stringOf } from "../shell/settingsModel.mjs";
import { AGENTS_KEY, DEFAULT_POLICY, DEFAULT_SNAPSHOT_WIDTH, ENABLED_KEY, SNAPSHOT_WIDTH_KEY, snapshotWidth } from "./drawSettingsModel.mjs";

export interface DrawPrefs {
  /** The settings have been read once; before that the defaults stand. */
  readonly read: boolean;
  readonly enabled: boolean;
  readonly agents: string;
  readonly snapshotWidth: number;
}

const DEFAULTS: DrawPrefs = { read: false, enabled: true, agents: DEFAULT_POLICY, snapshotWidth: DEFAULT_SNAPSHOT_WIDTH };

let prefs: DrawPrefs = DEFAULTS;
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The prefs from the resolved settings — the workspace's, since the canvas is the window's. */
export function drawPrefsOf(resolved: readonly ResolvedSetting[] | null): DrawPrefs {
  if (!resolved) return DEFAULTS;
  return {
    read: true,
    enabled: boolOf(resolved, ENABLED_KEY, true),
    agents: stringOf(resolved, AGENTS_KEY, DEFAULT_POLICY),
    snapshotWidth: snapshotWidth(resolved.find((r) => r.key === SNAPSHOT_WIDTH_KEY)?.value),
  };
}

export function setDrawPrefs(next: DrawPrefs): void {
  const same = (Object.keys(next) as (keyof DrawPrefs)[]).every((k) => next[k] === prefs[k]);
  if (same) return;
  prefs = next;
  for (const l of listeners) l();
}

export function drawPrefs(): DrawPrefs {
  return prefs;
}

export function useDrawPrefs(): DrawPrefs {
  return useSyncExternalStore(subscribe, () => prefs, () => prefs);
}

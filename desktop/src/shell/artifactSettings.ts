/**
 * The one setting a page artifact reads (ide/12): `artifacts.html.libraries`
 * — whether a page may load scripts, styles and fonts from the three public
 * CDNs. Read from the resolved workspace layer once per mount and on every
 * settings change; `true` until the node has answered, which is the
 * registry's default.
 */

import { useEffect, useState } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";

const LIBRARIES_KEY = "artifacts.html.libraries";

let cached: boolean | null = null;
const listeners = new Set<(v: boolean) => void>();

async function load(): Promise<void> {
  try {
    const { settings } = await api.settingsResolved(null);
    const row = settings.find((r) => r.key === LIBRARIES_KEY);
    const value = row ? row.value !== false : true;
    cached = value;
    for (const l of listeners) l(value);
  } catch {
    // The default stands until the node answers.
  }
}

export function useArtifactLibraries(): boolean {
  const [value, setValue] = useState<boolean>(cached ?? true);
  useEffect(() => {
    listeners.add(setValue);
    if (cached === null) void load();
    else setValue(cached);
    return () => {
      listeners.delete(setValue);
    };
  }, []);
  useEngineEvents((ev) => {
    if (ev.payload.type === "settings_changed") void load();
  });
  useReloadOnReconnect(() => void load());
  return value;
}

/**
 * Which dimension each footer overlay shows — the resources' per metric,
 * the browser's under `browser` — the person's choice, window furniture in
 * `localStorage` under one key, the way the footer remembers which
 * harness's usage it shows (`footerUsageStore.ts`). Whether the choice
 * still applies is `resourceModel.chosenDimension`'s call, and
 * `browserStatModel.chosenDimension`'s; this file only keeps.
 */

import { useSyncExternalStore } from "react";
import { jsonPref, readPref, webStorage, writePref } from "./storedPrefModel.mjs";

const KEY = "bisa.footer.resources";

type Choices = Readonly<Record<string, string>>;

function read(): Choices {
  const parsed = readPref(webStorage(), KEY, jsonPref, null);
  if (!parsed || typeof parsed !== "object") return {};
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) if (typeof v === "string") out[k] = v;
  return out;
}

let choices: Choices = read();
const listeners = new Set<() => void>();

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** The remembered dimension for a metric, as a subscription; null when the person never chose. */
export function useChosenDimension(metric: string): string | null {
  return useSyncExternalStore(subscribe, () => choices[metric] ?? null, () => null);
}

/** Remember a metric's dimension. */
export function chooseDimension(metric: string, dimension: string): void {
  if (choices[metric] === dimension) return;
  choices = { ...choices, [metric]: dimension };
  writePref(webStorage(), KEY, choices);
  for (const l of listeners) l();
}

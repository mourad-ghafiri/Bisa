/**
 * Which harnesses this node knows about, shared by every picker and by the
 * terminal launcher.
 *
 * No picker in the app joined an agent's `harness` against `HarnessRow.installed`,
 * so an agent whose harness is not installed here looked identical to a working
 * one everywhere you might address it: you picked it, you sent, and nothing
 * came back — with no surface anywhere saying why.
 *
 * Module-level rather than per-mount, and deliberately not folded into
 * {@link useWorkspaceState}: the workspace store reloads on a burst of bus
 * frames, and this list changes when somebody installs a CLI, which is not
 * something the message bus reports. So it is fetched once per window and
 * shared by every mount — and read again only for a reason: the bus coming
 * back after the node was away (`reloadOnReconnect`), a read that failed or
 * found no harness a person could launch (`harnessListModel.nextCatalogRead`:
 * a launch can outrun the node, and a cold `--version` can outstay its probe,
 * which is "not installed" for nobody), or a person pressing Refresh on the
 * footer's usage (`reloadHarnesses`).
 *
 * **Unknown is not the same as uninstalled.** Until the fetch lands the answer
 * is `null` and no row is marked — marking every agent as unavailable for the
 * eighty milliseconds before the response arrives would be a worse lie than
 * saying nothing, and it would flash on every dialog open. Callers must treat
 * `null` as "mark nothing", never as an empty list.
 */

import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { watchConnection } from "../bus";
import { errorFields, log } from "../log";
import type { HarnessRow } from "../types";
import { nextCatalogRead } from "./harnessListModel.mjs";
import { reloadOnReconnect } from "./workspaceLoadModel.mjs";

/** Every row the node reported, or `null` while the answer is still unknown. */
type Known = HarnessRow[] | null;

let cache: Known = null;
let inflight: Promise<void> | null = null;
const listeners = new Set<(next: Known) => void>();
/** How many re-reads followed the last read that answered nothing useful. */
let attempts = 0;
let retry: ReturnType<typeof setTimeout> | null = null;
let watching = false;

function publish(next: Known) {
  cache = next;
  for (const l of listeners) l(next);
}

/** A read that found no installed harness to offer is asked again, on the model's backoff; one that did ends the retries. */
function scheduleRetry() {
  if (retry) clearTimeout(retry);
  retry = null;
  const wait = nextCatalogRead(cache, attempts);
  if (wait === null) {
    if (cache !== null) attempts = 0;
    return;
  }
  retry = setTimeout(() => {
    retry = null;
    attempts += 1;
    void load();
  }, wait);
}

function load() {
  if (inflight) return inflight;
  inflight = api
    .harnesses()
    .then((r) => publish(r.harnesses))
    .catch((e: unknown) => {
      // The answer stays unknown, which marks nothing: an agent wrongly flagged
      // as unrunnable would be the picker inventing a problem.
      log.debug("harnesses", "the catalog could not be read; the answer stays unknown and marks nothing", errorFields(e));
    })
    .finally(() => {
      inflight = null;
      scheduleRetry();
    });
  return inflight;
}

/** The bus coming back after the node was away: the list is read again whole, as every store's is. */
function ensureWatching() {
  if (watching) return;
  watching = true;
  reloadOnReconnect(watchConnection, () => {
    attempts = 0;
    void load();
  });
}

/** Read which harnesses are installed again, now — what Refresh on the footer's usage asks first. */
export function reloadHarnesses(): void {
  attempts = 0;
  void load();
}

/** The whole catalog, or `null` until it is known. */
export function useHarnesses(): Known {
  const [rows, setRows] = useState<Known>(cache);

  useEffect(() => {
    listeners.add(setRows);
    ensureWatching();
    if (cache === null) void load();
    else setRows(cache);
    return () => {
      listeners.delete(setRows);
    };
  }, []);

  return rows;
}

/**
 * Installed harness ids, or `null` until known.
 *
 * Kept as its own hook because that is what the three agent pickers want, and
 * a `Set` is the shape their membership tests are written against.
 */
export function useInstalledHarnesses(): Set<string> | null {
  const rows = useHarnesses();
  if (rows === null) return null;
  return new Set(rows.filter((h) => h.installed).map((h) => h.id));
}

/**
 * Harness id → its display name, for surfaces that have an id and want a word.
 *
 * Empty rather than `null` while unknown: a terminal tab has to be labelled
 * *now*, and the id is a perfectly good fallback that happens to be what the
 * user typed in the first place.
 */
export function useHarnessLabels(): Record<string, string> {
  const rows = useHarnesses();
  // Memoised on the rows: a fresh object per render would defeat every memo
  // downstream that lists it as a dependency.
  return useMemo(() => {
    const labels: Record<string, string> = {};
    for (const h of rows ?? []) labels[h.id] = h.label;
    return labels;
  }, [rows]);
}

/**
 * The harnesses that can be opened in a terminal, installed ones first.
 *
 * A row with no `launch` is left out entirely rather than shown as disabled:
 * every `acp:*` adapter and every `a2a:*` target has no interactive form at all
 * — its command speaks a protocol — so it is not something a person could ever
 * be offered, as opposed to something they could install. Ones that *do* have a
 * form but are not on this machine stay in the list, disabled, because there
 * the install hint is the useful thing to say.
 */
export function useLaunchableHarnesses(): HarnessRow[] {
  const rows = useHarnesses();
  return useMemo(() => launchable(rows ?? []), [rows]);
}

function launchable(rows: readonly HarnessRow[]): HarnessRow[] {
  return rows
    .filter((h) => h.launch !== null)
    .sort((a, b) => {
      if (a.installed !== b.installed) return a.installed ? -1 : 1;
      return a.label.localeCompare(b.label);
    });
}

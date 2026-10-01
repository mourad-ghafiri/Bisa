/**
 * One fetch hook for the work screens.
 *
 * Every load carries an AbortSignal and every unmount cancels it, so a screen
 * that navigates away mid-flight never lands state on a dead component — the
 * failure mode that produces phantom errors and stale boards.
 *
 * A read may be **kept** for the life of the window (`keep`, a key from
 * `keptReadsModel.readKey`): a screen that comes back draws its last answer
 * at once and reads again behind it, so nothing stands empty for a frame
 * and a scroll has content to land on. A kept read is its key's: when the
 * key changes — the same screen drawn for another thing — what is shown is
 * that thing's last answer or nothing, never the previous thing's.
 *
 * Every read is **read again when the bus comes back** after the node was
 * away (`ui/useReloadOnReconnect`): what a node forgot in a restart and what
 * it did at boot reach a screen by no frame, and a read that found nobody
 * while the node was away mends itself once it is back — no screen waits for
 * a person to press *Try again*.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError } from "../../api";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { keepRead, keptRead } from "./keptReadsStore";

export interface Async<T> {
  data: T | null;
  error: string | null;
  /** True only on the first load; refreshes keep the old data on screen. */
  loading: boolean;
  /**
   * A load after the first is in flight: the old data stays on screen and a
   * surface may say *checking again…* beside it. False on the first load
   * (that is `loading`) and once the re-read settles.
   */
  refreshing: boolean;
  /** The node is unreachable rather than refusing — worth saying differently. */
  offline: boolean;
  /** The node says there is no such thing (a 404): it is gone, or never was. */
  missing: boolean;
  /** Unix seconds of the last load that answered; `null` until one has. */
  at: number | null;
  reload: () => void;
}

/** What a read may be asked for beyond its loader and its deps. */
export interface AsyncOptions {
  /** The key the answer is kept under for the life of the window; none keeps nothing. */
  keep?: string | null;
}

/** An answer, and the key it was read under — so another key's answer is never shown as this one's. */
interface Held<T> {
  key: string | null;
  data: T | null;
}

export function useAsync<T>(
  load: (signal: AbortSignal) => Promise<T>,
  deps: readonly unknown[],
  options?: AsyncOptions,
): Async<T> {
  const key = options?.keep ?? null;
  const [held, setHeld] = useState<Held<T>>(() => ({ key, data: keptRead<T>(key) ?? null }));
  const [error, setError] = useState<string | null>(null);
  const [offline, setOffline] = useState(false);
  const [missing, setMissing] = useState(false);
  const [loading, setLoading] = useState(() => held.data === null);
  const [refreshing, setRefreshing] = useState(false);
  const [at, setAt] = useState<number | null>(null);
  const [nonce, setNonce] = useState(0);
  const fn = useRef(load);
  fn.current = load;
  // Whether a load has answered before: the next one is a refresh, not the first.
  const answered = useRef(held.data !== null);

  // Another key is another thing: what is drawn is its own last answer, or
  // nothing — derived here, in the render that changed the key, so the
  // previous thing's answer is never on screen under this one's name.
  const mine = held.key === key;
  const data = mine ? held.data : (keptRead<T>(key) ?? null);

  useEffect(() => {
    const ac = new AbortController();
    let live = true;
    const kept = keptRead<T>(key) ?? null;
    setHeld((was) => (was.key === key ? was : { key, data: kept }));
    if (!mine) {
      answered.current = kept !== null;
      setLoading(kept === null);
      setError(null);
      setMissing(false);
    }
    if (answered.current) setRefreshing(true);
    void (async () => {
      try {
        const next = await fn.current(ac.signal);
        if (!live) return;
        answered.current = true;
        keepRead(key, next);
        setHeld({ key, data: next });
        setAt(Math.floor(Date.now() / 1000));
        setError(null);
        setOffline(false);
        setMissing(false);
      } catch (e) {
        if (!live || ac.signal.aborted) return;
        setError(e instanceof Error ? e.message : String(e));
        setOffline(e instanceof ApiError && e.offline);
        setMissing(e instanceof ApiError && e.status === 404);
      } finally {
        if (live) {
          setLoading(false);
          setRefreshing(false);
        }
      }
    })();
    return () => {
      live = false;
      ac.abort();
    };
    // `load` is held in a ref; the caller's deps decide when to refetch, and
    // `mine` is read for the run the key's change started.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce, key]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  useReloadOnReconnect(reload);
  // Until the effect has caught up with a key that changed, what is said is
  // the new thing's: still loading when nothing of it is kept, and none of
  // the previous thing's error.
  if (!mine) return { data, error: null, loading: data === null, refreshing: data !== null, offline: false, missing: false, at: null, reload };
  return { data, error, loading: loading && data === null, refreshing, offline, missing, at, reload };
}

/**
 * Run a mutation, surfacing failures as a toast rather than a dead click.
 * `onDone` is handed what the call answered, for the callers that show it —
 * a generated key, a check's result; most ignore it and reload.
 */
export async function attempt<T>(
  fn: () => Promise<T>,
  onError: (message: string) => void,
  onDone?: (value: T) => void,
): Promise<boolean> {
  try {
    const value = await fn();
    onDone?.(value);
    return true;
  } catch (e) {
    onError(e instanceof Error ? e.message : String(e));
    return false;
  }
}

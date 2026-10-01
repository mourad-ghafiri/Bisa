/**
 * The thumbnails a run keeps (ide/14 §Photos): one small square per photo,
 * made once from the stored bytes whatever their size — a photo set before
 * scaling, a blob a peer synced — and handed to every draw site as the same
 * object URL, so five sites drawing one project decode one bitmap and a
 * workspace reload redraws from here without a fetch. Bounded: past
 * `THUMB_CACHE_MAX` the least recently drawn are let go (`evictOrder`). A
 * photo that cannot be made is `null` for the run, never retried in a loop.
 */

import { useEffect, useSyncExternalStore } from "react";
import { api } from "../api";
import { THUMB_CACHE_MAX, THUMB_EDGE, evictOrder, thumbKey } from "./photoModel.mjs";
import { thumbOf } from "./photoScale";

interface Entry {
  url: string | null;
  at: number;
}

const thumbs = new Map<string, Entry>();
const pending = new Set<string>();
const listeners = new Set<() => void>();
let tick = 0;

function publish(): void {
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

function trim(): void {
  const gone = evictOrder(
    [...thumbs.entries()].map(([key, e]) => ({ key, at: e.at })),
    THUMB_CACHE_MAX,
  );
  for (const key of gone) {
    const url = thumbs.get(key)?.url;
    if (url) URL.revokeObjectURL(url);
    thumbs.delete(key);
  }
}

/** Start the one fetch and scale for a photo, unless it is here or on its way. */
function ensure(sha: string, edge: number): void {
  const key = thumbKey(sha, edge);
  if (thumbs.has(key) || pending.has(key)) return;
  pending.add(key);
  void thumbOf(api.attachmentUrl(sha, { image: true }), edge)
    .then((url) => {
      thumbs.set(key, { url, at: ++tick });
      trim();
    })
    .catch(() => {
      thumbs.set(key, { url: null, at: ++tick });
    })
    .finally(() => {
      pending.delete(key);
      publish();
    });
}

/** The thumbnail's object URL, when it is here; a read marks it as drawn. */
function read(sha: string | null, edge: number): string | null {
  if (!sha) return null;
  const e = thumbs.get(thumbKey(sha, edge));
  if (!e) return null;
  e.at = ++tick;
  return e.url;
}

/**
 * The small square for a photo, shared by every draw site; `null` while it
 * is made, when there is no photo, or when the bytes could not be read.
 */
export function usePhotoThumb(sha: string | null | undefined, edge: number = THUMB_EDGE): string | null {
  const key = sha ?? null;
  const url = useSyncExternalStore(
    subscribe,
    () => read(key, edge),
    () => null,
  );
  useEffect(() => {
    if (key) ensure(key, edge);
  }, [key, edge]);
  return url;
}

/** Forget a photo's thumbnail — after its bytes arrived from a peer, so the next draw makes it. */
export function forgetPhotoThumb(sha: string): void {
  for (const key of [...thumbs.keys()]) {
    if (key.startsWith(`${sha}@`)) {
      const url = thumbs.get(key)?.url;
      if (url) URL.revokeObjectURL(url);
      thumbs.delete(key);
    }
  }
  publish();
}

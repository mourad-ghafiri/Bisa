/**
 * The bytes an artifact viewer draws from (ide/12).
 *
 * Fetched from the node with the bearer header — never a URL a frame or an
 * `<img>` is pointed at: the node serves nothing an agent wrote as a page,
 * and a viewer that holds the bytes can hand them to a sandboxed frame, a
 * PDF renderer, a spreadsheet parser or a blob URL as it needs. Blob URLs
 * are kept in one bounded cache and revoked when they fall out of it, so
 * scrolling a long conversation of pictures does not hold every one.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";

export interface Loaded {
  url: string;
  bytes: Uint8Array;
}

export type BytesState =
  | { state: "absent" }
  | { state: "loading" }
  | { state: "ready"; url: string; bytes: Uint8Array }
  | { state: "failed"; error: string };

const CACHE_BYTES = 200 * 1024 * 1024;
const cache = new Map<string, Loaded>();
const inflight = new Map<string, Promise<Loaded>>();
let held = 0;

function remember(sha256: string, bytes: Uint8Array, mime: string): Loaded {
  const loaded = { url: URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime })), bytes };
  cache.set(sha256, loaded);
  held += bytes.byteLength;
  // Oldest first: a Map iterates in insertion order.
  for (const [key, old] of cache) {
    if (held <= CACHE_BYTES) break;
    if (key === sha256) continue;
    cache.delete(key);
    held -= old.bytes.byteLength;
    URL.revokeObjectURL(old.url);
  }
  return loaded;
}

/** The bytes of one blob, cached across viewers by hash. */
export function loadArtifactBytes(sha256: string, mime: string): Promise<Loaded> {
  const hit = cache.get(sha256);
  if (hit) return Promise.resolve(hit);
  const pending = inflight.get(sha256);
  if (pending) return pending;
  const p = api
    .attachmentBytes(sha256)
    .then((bytes) => remember(sha256, bytes, mime))
    .finally(() => inflight.delete(sha256));
  inflight.set(sha256, p);
  return p;
}

/** The bytes as text — a page, a sheet, a document source. */
export function textOf(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes);
}

export function useArtifactBytes(sha256: string, mime: string, present: boolean): BytesState {
  const [state, setState] = useState<BytesState>(() => {
    if (!present) return { state: "absent" };
    const hit = cache.get(sha256);
    return hit ? { state: "ready", ...hit } : { state: "loading" };
  });
  useEffect(() => {
    if (!present) {
      setState({ state: "absent" });
      return;
    }
    let live = true;
    const hit = cache.get(sha256);
    if (hit) {
      setState({ state: "ready", ...hit });
      return;
    }
    setState({ state: "loading" });
    loadArtifactBytes(sha256, mime)
      .then((loaded) => live && setState({ state: "ready", ...loaded }))
      .catch((e: unknown) => live && setState({ state: "failed", error: e instanceof Error ? e.message : String(e) }));
    return () => {
      live = false;
    };
  }, [sha256, mime, present]);
  return state;
}

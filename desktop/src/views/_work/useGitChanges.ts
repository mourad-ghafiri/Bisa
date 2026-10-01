/**
 * The Git tab's ear on the checkout (ide/04 §Live): every `file_changed`
 * frame for one workstream's root, said as the kinds of git change it is
 * (`gitChangeModel.kindsOf`), gathered for `GIT_COALESCE_MS` and handed to
 * the caller once per burst — so a checkout touching fifty files, a fetch
 * writing twenty refs, or the platform's own verb writing `index` then
 * `HEAD` is one re-read per view. The handler rides a ref, the timer is
 * cleared on unmount, and a `rescan` frame is every kind at once.
 */

import { useEffect, useRef } from "react";
import { useBus } from "../../bus";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { GIT_CHANGES, GIT_COALESCE_MS, kindsOf } from "./gitChangeModel.mjs";
import type { GitChange } from "./gitChangeModel.mjs";

export function useGitChanges(wid: string, handler: (kinds: Set<GitChange>) => void): void {
  const latest = useRef(handler);
  latest.current = handler;
  const gathered = useRef<Set<GitChange>>(new Set());
  const flush = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (flush.current !== null) clearTimeout(flush.current);
      flush.current = null;
      gathered.current = new Set();
    },
    [wid],
  );

  // The node was away and its watcher with it: whatever moved meanwhile
  // came by no frame, so every read is owed.
  useReloadOnReconnect(() => latest.current(new Set(GIT_CHANGES)));

  useBus({ stream: "engine" }, (f) => {
    if (f.stream !== "engine") return;
    const p = f.payload.payload;
    if (p.type !== "file_changed" || p.scope !== "workstream" || p.id !== wid) return;
    const kinds = kindsOf(p);
    if (kinds.size === 0) return;
    for (const k of kinds) gathered.current.add(k);
    if (flush.current !== null) return;
    flush.current = setTimeout(() => {
      flush.current = null;
      const burst = gathered.current;
      gathered.current = new Set();
      latest.current(burst);
    }, GIT_COALESCE_MS);
  });
}

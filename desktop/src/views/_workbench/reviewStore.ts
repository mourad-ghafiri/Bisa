/**
 * The change ledger for the conversation a checkout is on (ide/09 §Modes and
 * the change ledger), kept fresh and shared: one `GET /conversations/{id}/changes`
 * per conversation, refetched on `changes_moved` and `changes_settled` and when
 * the node comes back after being away — never polled — and republished per checkout so a tab or an explorer row can wear
 * a *to review* dot without each holding its own read.
 *
 * `useConversationChanges` is the read; `usePendingReviewPaths` is what a
 * root's furniture subscribes to. The two are kept apart because a document
 * far from the conversation pane (a tab, a Files row) needs the paths alone,
 * not the whole card-shaped view.
 */

import { useEffect, useSyncExternalStore } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import type { ChangesView } from "../../types";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { useAsync } from "../_work/useAsync";
import { pendingPathsOf } from "../_studio/turnChangesModel.mjs";
import { useCoalesced } from "./useCoalesced";

/** One conversation's changes, kept fresh: re-read on `changes_moved` / `changes_settled` for it. */
export function useConversationChanges(conversationId: string | null) {
  const q = useAsync(
    (s) => (conversationId ? api.conversationChanges(conversationId, s) : Promise.resolve(null)),
    [conversationId],
  );
  const reload = useCoalesced(q.reload);
  useReloadOnReconnect(reload);
  useEngineEvents((e) => {
    const p = e.payload;
    if ((p.type === "changes_moved" || p.type === "changes_settled") && conversationId && p.conversation === conversationId) reload();
  });
  return q;
}

// --- republished per checkout, for furniture far from the pane ------------

interface Entry {
  paths: ReadonlySet<string>;
}

const EMPTY: Entry = { paths: new Set() };
const byWorkstream = new Map<string, Entry>();
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** Republishes a checkout's pending paths — called from `usePublishReviewChanges` alone. */
function publishReviewChanges(wid: string, view: ChangesView | null | undefined): void {
  const next: Entry = { paths: pendingPathsOf(view) };
  const cur = byWorkstream.get(wid);
  if (cur && cur.paths.size === next.paths.size && [...cur.paths].every((p) => next.paths.has(p))) return;
  byWorkstream.set(wid, next);
  emit();
}

/** The paths still pending in the checkout's conversation — a tab or an explorer row's dot. */
export function usePendingReviewPaths(wid: string): ReadonlySet<string> {
  const entry = useSyncExternalStore(
    subscribe,
    () => byWorkstream.get(wid) ?? EMPTY,
    () => EMPTY,
  );
  return entry.paths;
}

/**
 * Wires a checkout's live changes into the republished store for its
 * lifetime — call once, from `useConversationPane`.
 */
export function usePublishReviewChanges(wid: string, view: ChangesView | null | undefined): void {
  useEffect(() => {
    publishReviewChanges(wid, view);
  }, [wid, view]);
}

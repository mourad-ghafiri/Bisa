/**
 * Conversations as the desktop reads them (13 — Conversations): the lists a
 * surface reads and the one record it is on, moved by the bus; starting one;
 * and where a hand-off from the editor lands.
 *
 * Every list is the node's (`GET /conversations`), re-read when the engine
 * says a conversation was started or changed, when a message lands in one
 * the list holds, and when the node comes back after being away — so a
 * count, a first line or a title never lags. Which conversation an owner is
 * on is the pick store's (`_studio/conversationPickStore.ts`), one memory for
 * every owner; the rules — the order, what a filter admits, where a hand-off
 * goes — are `conversationsModel.mjs`, `conversationSurfaceModel.mjs` and
 * `conversationPaneModel.mjs`; this file only fetches.
 */

import { useMemo } from "react";
import { ApiError, api } from "../../api";
import { useConversationEvents, useEngineEvents } from "../../bus";
import type { ConversationOrigin, ConversationView } from "../../types";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { useAsync } from "../_work/useAsync";
import { keepPick, rememberedPick } from "../_studio/conversationPickStore";
import { PAGE } from "../_studio/conversationSurfaceModel.mjs";
import { sortConversations } from "../_studio/conversationsModel.mjs";
import { currentConversation, handOffTarget, rememberedStillHere } from "./conversationPaneModel.mjs";
import { rootKey } from "./workbenchModel.mjs";
import { useCoalesced } from "./useCoalesced";

// --- the lists -------------------------------------------------------------

export interface ConversationsQuery {
  origin?: ConversationOrigin["kind"];
  id?: string;
  /** Every conversation standing in one project — instead of `origin`. */
  project?: string;
  agent?: string;
  q?: string;
  archived?: boolean;
  limit?: number;
}

/**
 * A page of conversations for a query, kept fresh: re-read on a
 * `conversation_created` / `conversation_changed` frame, and on a message in
 * one of the rows it holds.
 */
export function useConversations(query: ConversationsQuery, enabled = true) {
  const key = JSON.stringify(query);
  const list = useAsync(
    (s) => (enabled ? api.conversations(query, s) : Promise.resolve({ conversations: [] })),
    [key, enabled],
  );
  const reload = useCoalesced(list.reload);
  useReloadOnReconnect(reload);
  useEngineEvents((e) => {
    if (e.payload.type === "conversation_created" || e.payload.type === "conversation_changed") reload();
  });
  const ids = useMemo(() => new Set((list.data?.conversations ?? []).map((c) => c.id)), [list.data]);
  useConversationEvents((f) => {
    if (f.scope && ids.has(f.scope)) reload();
  });
  const rows = useMemo(() => sortConversations(list.data?.conversations ?? []), [list.data]);
  return { rows, loading: list.loading, refreshing: list.refreshing, error: list.error, reload: list.reload };
}

/**
 * One conversation by id, kept fresh the same way — what a surface is on,
 * whatever its list shows. `missing` is the node's *not found*: deleted under
 * the person, or a link to nothing.
 */
export function useConversation(id: string | null) {
  const one = useAsync((s) => (id ? api.conversation(id, s).then((r) => r.conversation) : Promise.resolve(null)), [id]);
  const reload = useCoalesced(one.reload);
  useReloadOnReconnect(reload);
  useEngineEvents((e) => {
    if ((e.payload.type === "conversation_changed" || e.payload.type === "conversation_created") && e.payload.id === id) reload();
  });
  useConversationEvents((f) => {
    if (id && f.scope === id) reload();
  });
  return one;
}

// --- starting one, and where a hand-off lands ---------------------------------

/**
 * Start a conversation about something — what *New conversation* does on
 * every surface, untitled unless a title is given. Where it is picked is the
 * caller's: the surface picks it through its selection, a hand-off through
 * the pick store.
 */
export async function startConversation(origin: ConversationOrigin, title?: string): Promise<ConversationView> {
  const { conversation } = await api.createConversation({ origin, ...(title ? { title } : {}) });
  return conversation;
}

/**
 * The conversation a hand-off from a checkout goes to: the one the checkout
 * is on when its turns run here — the remembered pick if it is still live
 * in this project, else the newest — and a new one about the workstream
 * otherwise (`handOffTarget`). A hand-off carries this checkout's files, so
 * it never lands in a sibling checkout's conversation, and never names a
 * session. Where it landed becomes the checkout's pick, so the Agent pane
 * stands on it.
 */
export async function ensureConversation(wid: string, pid: string): Promise<{ id: string; started: boolean }> {
  const root = rootKey("workstream", wid);
  const remembered = rememberedPick(root);
  let current: ConversationView | null = null;
  if (remembered) {
    try {
      const { conversation } = await api.conversation(remembered);
      if (rememberedStillHere(conversation, pid)) current = conversation;
    } catch (e) {
      // Gone — the node's *not found*: fall through to the newest, or a new
      // one. Any other failure is the node not answering, and a hand-off
      // that started a fresh conversation for it would leave two.
      if (!(e instanceof ApiError && e.status === 404)) throw e;
    }
  }
  if (!current) {
    const { conversations } = await api.conversations({ project: pid, archived: false, limit: PAGE });
    current = currentConversation(conversations, wid, pid, null);
  }
  const target = handOffTarget(current, wid, pid);
  if ("conversation" in target) {
    keepPick(root, target.conversation);
    return { id: target.conversation, started: false };
  }
  const made = await startConversation(target.start);
  keepPick(root, made.id);
  return { id: made.id, started: true };
}

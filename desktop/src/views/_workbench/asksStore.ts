/**
 * The open asks in a conversation about a checkout (ide/09, 11 — Security):
 * one `GET /conversations/{id}/asks` on mount, kept fresh by `ask_opened`
 * and `ask_settled` — no polling — and read again when the node comes back:
 * an ask lives as long as its session, so a restarted node holds none, and
 * no frame says so.
 */

import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { useAsync } from "../_work/useAsync";
import { useCoalesced } from "./useCoalesced";

export function useConversationAsks(conversationId: string | null) {
  const q = useAsync(
    (s) => (conversationId ? api.conversationAsks(conversationId, s) : Promise.resolve({ asks: [] })),
    [conversationId],
  );
  const reload = useCoalesced(q.reload);
  useReloadOnReconnect(reload);
  useEngineEvents((e) => {
    const p = e.payload;
    if (!conversationId) return;
    if (p.type === "ask_opened" && p.conversation === conversationId) reload();
    if (p.type === "ask_settled" && p.conversation === conversationId) reload();
  });
  return q;
}

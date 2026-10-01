/**
 * A thread reads itself as it is shown (13-conversations): the one hook the
 * chat calls, wherever it is mounted — the IDE's Agent pane, a goal's or a
 * workflow's conversation, a channel, a direct message, the Inbox's own
 * pane, a hosted channel — so no route has to know what is on screen. When
 * the scope has something to read (`readWanted`) and the person can see the
 * newest message (`shown`), the scope is marked read after a beat; a change
 * of scope, of what is unread or of what is shown restarts the beat, so a
 * row passed on the way reads nothing and a reply that lands while you look
 * is read as it lands. The words and the rules are `readModel.mjs`'s; the
 * mark is the workspace store's (`markRead`), which patches what the shell
 * shows before the node confirms it.
 */

import { useEffect } from "react";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { READ_AFTER_MS, readWanted } from "./readModel.mjs";

export function useReadAsShown({
  scope,
  host,
  unreadCount,
  shown,
}: {
  /** The conversation's scope id. */
  scope: string;
  /** The host's key for a hosted scope, else `null`. */
  host: string | null;
  /** How many messages are unread here, as the caller knows them — this node's map, or the hosted entry's count. */
  unreadCount: number;
  /** Whether the person can see the newest message right now. */
  shown: boolean;
}): void {
  const ws = useWorkspace();
  const row = host ? null : (ws.inbox.find((r) => r.key === scope) ?? null);
  const wanted = readWanted(unreadCount, row);
  const markRead = ws.markRead;
  useEffect(() => {
    if (!wanted || !shown) return;
    const t = window.setTimeout(() => markRead(scope, host), READ_AFTER_MS);
    return () => window.clearTimeout(t);
  }, [wanted, shown, scope, host, markRead]);
}

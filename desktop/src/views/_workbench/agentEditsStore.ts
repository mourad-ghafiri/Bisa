/**
 * The edits handed to agents, per document, for the life of the window: the
 * annotation tray and the selection toolbar record one when their request
 * lands, and a document that is open hears when the agent it named came to
 * rest — through the roster's transitions (`shell/sessionsStore.ts`) and
 * the rule in `agentEditsModel.mjs` — so it reads the file again and says
 * so, watcher frame or not. One record per document; a second ask for the
 * same file replaces the first; a settled one is forgotten.
 */

import { useEffect, useRef } from "react";
import { onSessionTransition } from "../../shell/sessionsStore";
import { stateOf } from "../../ui/sessionState.mjs";
import type { SessionRow } from "../../types";
import { recordKey, settles } from "./agentEditsModel.mjs";
import type { AgentEditRecord } from "./agentEditsModel.mjs";

const records = new Map<string, AgentEditRecord>();

/** An edit was asked of `agentId` for this document — remember it until the agent comes to rest. */
export function recordAgentEdit({ scope, id, path, agentId }: { scope: string; id: string; path: string; agentId: string }): void {
  records.set(recordKey(scope, id, path), { scope, id, agentId, at: Date.now() / 1000 });
}

/** What settled: the agent asked, the state it came to rest in, the row. */
export interface AgentEditSettled {
  agentId: string;
  state: string;
  row: SessionRow;
}

/** Hear the agent asked for this document come to rest. The latest handler is the one called. */
export function useAgentEditSettled(scope: string, id: string, path: string, onSettled: (settled: AgentEditSettled) => void): void {
  const latest = useRef(onSettled);
  latest.current = onSettled;
  useEffect(
    () =>
      onSessionTransition((prev, row) => {
        const key = recordKey(scope, id, path);
        const record = records.get(key);
        if (!record || !settles(record, prev, row)) return;
        records.delete(key);
        latest.current({ agentId: record.agentId, state: stateOf(row.state), row });
      }),
    [scope, id, path],
  );
}

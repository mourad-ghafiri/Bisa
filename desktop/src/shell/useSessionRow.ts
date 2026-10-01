/**
 * One session's roster row by id: from the live roster while it holds it,
 * else read once from the node (`GET /sessions/{id}`) — a transcript pane
 * or tab kept open after the roster dropped its session still names it.
 * `null` while nothing is known yet, or when the node knows nothing either.
 */

import { useEffect, useState } from "react";
import { api } from "../api";
import { errorFields, log } from "../log";
import type { SessionRow } from "../types";
import { useSessions } from "./sessionsStore";

export function useSessionRow(id: string): SessionRow | null {
  const live = useSessions().find((s) => s.id === id) ?? null;
  const [read, setRead] = useState<SessionRow | null>(null);
  const absent = live === null;
  useEffect(() => {
    setRead(null);
    if (!absent) return;
    const ac = new AbortController();
    api
      .session(id, ac.signal)
      .then((row) => !ac.signal.aborted && setRead(row))
      .catch((e: unknown) => {
        // Unknown to the node too: the words say the session is gone.
        if (ac.signal.aborted) return;
        log.debug("sessions", "the session could not be read; the words say it is gone", { id, ...errorFields(e) });
      });
    return () => ac.abort();
    // The roster's row wins whenever it exists; a read is only for its absence.
  }, [id, absent]);
  return live ?? read;
}

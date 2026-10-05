/**
 * The roster as the tabs say it (`terminalsModel.settledRoster`): every
 * session through `settledByTab` with the tab that claims it, live while
 * either store moves. The one roster every surface that marks or counts
 * sessions reads — the pet, the tray, the Agents screen, an addon's summary
 * — so a tab that exited settles its mark, its pill, its count and the
 * footer alike, and no two of them disagree about one session. The rail's
 * rows, the pulse line and the activity dot apply the same rule through
 * their own models, with the terminals they already hold.
 *
 * Its own module rather than a hook of `sessionsStore.ts`: the terminals
 * store imports that store (to close an aborted session's tab), and a hook
 * there importing the terminals store back would be a cycle evaluated
 * before either store's state exists.
 */

import { useMemo } from "react";
import type { SessionRow } from "../types";
import { useSessions } from "./sessionsStore";
import { settledRoster } from "./terminalsModel.mjs";
import { useTerminals } from "./useTerminals";

export function useSettledSessions(): readonly SessionRow[] {
  const sessions = useSessions();
  const { sessions: terminals } = useTerminals();
  return useMemo(() => settledRoster(sessions, terminals), [sessions, terminals]);
}

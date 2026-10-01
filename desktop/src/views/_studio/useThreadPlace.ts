/**
 * Where a thread was being read, kept across leaving it and across a restart
 * (`threadPlaceModel.mjs` has the rules; `threadPlaces` in
 * `viewMemoryStore.ts` is the memory). Keyed by the chat's own key —
 * `<kind>:<scope>`, as `Conversation.tsx` keys `Chat` — so a thread has one
 * place whichever screen shows it: the IDE's Agent pane, the goal's tab, the
 * Inbox.
 *
 * Kept quietly: a place is read once, when the thread mounts, so writing it
 * on every scroll re-renders nobody.
 */

import { useMemo } from "react";
import { threadPlaces } from "../../shell/viewMemoryStore";
import { parseThreadPlace, type ThreadPlace } from "./threadPlaceModel.mjs";

/** The one name a thread's place is kept under. */
const AT = "at";

export interface KeptThreadPlace {
  /** The place kept when the thread mounted; `null` for a thread left at its bottom, or never read. */
  kept: ThreadPlace | null;
  /** Keep where the thread is being read; `null` — at the bottom — keeps nothing. */
  keep: (place: ThreadPlace | null) => void;
  /** The place cannot be reached any more: forget it. */
  forget: () => void;
}

export function useThreadPlace(key: string): KeptThreadPlace {
  return useMemo(
    () => ({
      kept: parseThreadPlace(threadPlaces.read(key, AT)),
      keep: (place) => void threadPlaces.keepQuietly(key, AT, place),
      forget: () => void threadPlaces.forget(key),
    }),
    [key],
  );
}

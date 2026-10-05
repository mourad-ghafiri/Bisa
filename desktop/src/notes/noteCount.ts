/**
 * How many notes there are — the number the notes dock wears, read and kept
 * by `shell/liveCount.ts` whether or not the panel is open: every scope's
 * notes (`GET /notes` with no query, the *All* tab's own read), read again on
 * the frames `notesModel.movesNoteCount` names and when the bus comes back.
 */

import { api } from "../api";
import { createLiveCount } from "../shell/liveCount";
import { movesNoteCount } from "./notesModel.mjs";

const notes = createLiveCount({
  what: "notes",
  read: async (signal) => (await api.notes("", signal)).notes.length,
  moves: movesNoteCount,
});

/** How many notes there are, while something shows it — `null` until the first answer. */
export function useNoteCount(): number | null {
  return notes.useCount();
}

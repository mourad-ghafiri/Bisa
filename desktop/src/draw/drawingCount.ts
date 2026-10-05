/**
 * How many drawings there are (19 — Drawings) — the number the drawings dock
 * wears, read and kept by `shell/liveCount.ts` whether or not the panel is
 * open: every scope's drawings (`GET /drawings` with no query, the *All*
 * tab's own read), read again on the frames `drawModel.movesDrawingCount`
 * names and when the bus comes back.
 */

import { api } from "../api";
import { createLiveCount } from "../shell/liveCount";
import { movesDrawingCount } from "./drawModel.mjs";

const drawings = createLiveCount({
  what: "drawings",
  read: async (signal) => (await api.drawings("", signal)).drawings.length,
  moves: movesDrawingCount,
});

/** How many drawings there are, while something shows it — `null` until the first answer. */
export function useDrawingCount(): number | null {
  return drawings.useCount();
}

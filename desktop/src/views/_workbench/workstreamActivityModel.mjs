/**
 * What a workstream's sessions and shells add up to — the dot on a rail row,
 * and the project's dot as the fold of its workstreams.
 *
 * The vocabulary is `ui/sessionState.mjs`'s: the loudest state wins across
 * sessions (waiting beats failed beats done beats work beats idle). A harness
 * a person opened in a terminal is a roster session like any other — it
 * reports its own state, so it is already among `sessions` — and a shell adds
 * a word only when it died badly. Nothing here reads bytes: a live shell
 * sitting at its prompt says nothing, and a working dot is only ever a state
 * a session reported. Nothing here sorts rows — attention shows in the dot and
 * never reorders a list under the pointer.
 */

import { attentionRank, counts, loudest } from "../../ui/sessionState.mjs";
import { settledRoster } from "../../shell/terminalsModel.mjs";
import { claimedSessions, isDrawn } from "./workstreamSessionsModel.mjs";

/**
 * The one word a shell can say for itself: *failed* when it exited badly,
 * else nothing (`null`). Alive, quiet, exited cleanly, lost contact — none of
 * those is a state a person reads; what the harness in the shell is doing
 * arrives as a roster session, never from here.
 * @param {import("../../shell/terminalsModel.mjs").TerminalSessionState} s
 * @returns {"failed" | null}
 */
export function terminalState(s) {
  const exitedBadly = s?.liveness?.status === "exited" && s.liveness.code !== 0 && s.liveness.code !== null;
  return exitedBadly ? "failed" : null;
}

/** The loudest of several state words. */
export function fold(words) {
  let best = "idle";
  for (const w of words ?? []) if (attentionRank(w) < attentionRank(best)) best = w;
  return best;
}

/**
 * Everything standing in one workstream, folded.
 * @param {import("../../types").SessionRow[]} sessions
 * @param {import("../../shell/terminalsModel.mjs").TerminalSessionState[]} terminals
 * @param {string} workstream
 */
export function workstreamActivity(sessions, terminals, workstream) {
  const claimed = claimedSessions(sessions, terminals);
  // As the claiming tab says (`settledRoster`): a tab that exited ended its
  // session, so the dot settles with the pill and the footer, not after them.
  const here = settledRoster(sessions ?? [], terminals ?? []).filter((s) => s.workstream === workstream && isDrawn(s, claimed));
  const shells = (terminals ?? []).filter((t) => t.scope === "workstream" && t.id === workstream);
  // A tab a roster row claims speaks through that row; only an unclaimed
  // shell's own exit is a word here.
  const shellWords = shells
    .filter((t) => !(t.sessionId && claimed.has(t.sessionId)))
    .map(terminalState)
    .filter(Boolean);
  const state = fold([loudest(here), ...shellWords]);
  const c = counts(here);
  return {
    state,
    counts: {
      needsYou: c.waiting + c.failed + shellWords.length,
      working: c.working,
      done: c.done,
      live: shells.filter((t) => t.liveness?.status === "live").length,
      agents: here.length,
    },
  };
}

/** A project's dot is the fold of its workstreams'. */
export function projectActivity(byWorkstream) {
  return {
    state: fold((byWorkstream ?? []).map((a) => a.state)),
    counts: (byWorkstream ?? []).reduce(
      (acc, a) => ({
        needsYou: acc.needsYou + a.counts.needsYou,
        working: acc.working + a.counts.working,
        done: acc.done + a.counts.done,
        live: acc.live + a.counts.live,
        agents: acc.agents + a.counts.agents,
      }),
      { needsYou: 0, working: 0, done: 0, live: 0, agents: 0 },
    ),
  };
}

/**
 * How long a session's mark dwells on a working word before another working
 * word may replace it (ide/09 §The marks). A harness mid-turn flips between
 * *thinking* and *running* on every tool call — several times a second on a
 * quick one — and a mark that followed each flip restarted its motion each
 * time: the breath and the spin cut into each other and read as a flicker,
 * not as work. So a working word holds for a beat; a word that asks for
 * attention, or ends the session, shows at once — nothing here delays a
 * raised hand or a failure.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it. The
 * timer is the component's (`ui/SessionMark.tsx`); the rule is here.
 */

import { stateOf } from "./sessionState.mjs";

/** The beat a working word holds the mark — long enough to be one motion, short enough to still be live. */
export const DWELL_MS = 300;

const WORKING = Object.freeze(["thinking", "running", "starting"]);

/**
 * Whether a state is one of the working words — the ones a tool call flips.
 * @param {import("./sessionState.mjs").StateLike} state
 */
function isWorking(state) {
  return WORKING.includes(stateOf(state));
}

/**
 * What the mark shows for a state that just arrived, given the one it shows
 * and how long that one has held: the new state, unless both are working
 * words and the shown one has not held for {@link DWELL_MS} yet — then the
 * shown one still, with how long until the new one may show. The same word
 * in a new object (another tool under *running*) is the new object at once:
 * same glyph, same motion, nothing restarts.
 * @template S
 * @param {S | null | undefined} shown what the mark shows
 * @param {S} next what the roster now says
 * @param {number} heldMs how long `shown` has been showing
 * @returns {{state: S, holdMs: number}} what to show, and after how many ms to look again (0: settled)
 */
export function shownState(shown, next, heldMs) {
  if (shown === null || shown === undefined) return { state: next, holdMs: 0 };
  if (stateOf(shown) === stateOf(next)) return { state: next, holdMs: 0 };
  if (isWorking(shown) && isWorking(next) && heldMs < DWELL_MS) return { state: shown, holdMs: DWELL_MS - heldMs };
  return { state: next, holdMs: 0 };
}

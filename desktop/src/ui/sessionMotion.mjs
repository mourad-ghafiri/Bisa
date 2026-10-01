/**
 * How a session's state mark moves (ide/09): one rule per word, so the rail,
 * the session lists and the Agents screen animate the same way — and so the
 * amount of motion is decided here, once, rather than glyph by glyph.
 *
 * Live states move a little, settled states not at all, and the two that
 * arrive — *done* and *failed* — move once on arrival and then stand still.
 * *Waiting* is the one state allowed to interrupt, and it does so with a nudge
 * every few seconds rather than a strobe: attention, not alarm. Every motion
 * is a `motion-*` class in `theme/motion.css`, where reduced motion stops all
 * of them dead.
 */

import { stateOf } from "./sessionState.mjs";

/** Every motion a mark can carry. */
export const MOTIONS = Object.freeze(["spin", "breathe", "nudge", "pop", "shake", "none"]);

const BY_WORD = Object.freeze({
  starting: "spin",
  running: "spin",
  thinking: "breathe",
  waiting: "nudge",
  done: "pop",
  failed: "shake",
  aborted: "none",
  idle: "none",
  parked: "none",
});

/**
 * The motion for a state.
 * @param {import("./sessionState.mjs").StateLike} state
 * @returns {"spin" | "breathe" | "nudge" | "pop" | "shake" | "none"}
 */
export function motionOf(state) {
  return BY_WORD[stateOf(state)] ?? "none";
}

/** Repeats while the state holds, as opposed to playing once on arrival. */
export function repeats(motion) {
  return motion === "spin" || motion === "breathe" || motion === "nudge";
}

/** The class `theme/motion.css` defines for a motion, or none. */
export function motionClass(motion) {
  return motion === "none" ? "" : `motion-${motion}`;
}

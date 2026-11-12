/**
 * What the node answered a write, held until the list it changed is read
 * again.
 *
 * A control that writes a whole list — an agent's skills, a goal's
 * assignees, a team's roster — draws the list a read gave it. Between the
 * write's answer and the next read that list is one write behind: release
 * the control then, and a quick second change is built from the stale list
 * and takes the first change back with it. So the answer is held beside the
 * list **it was an answer over**, and stands for exactly as long as that
 * same list is still the one in hand: the moment a read brings another, the
 * read is the truth again and the held answer is nothing.
 *
 * Identity, not equality: a read that brings an equal list is still a newer
 * word than the answer.
 */

/**
 * @template L, A
 * @param {L} listed the list a read gave
 * @param {{over: L, answer: A} | null | undefined} held an answer and the list it was made over
 * @returns {A | null} the answer, while `listed` is still the list it was made over
 */
export function heldFor(listed, held) {
  return held != null && held.over === listed ? held.answer : null;
}

/** @template L, A @param {L} listed @param {A} answer @returns {{over: L, answer: A}} */
export function holding(listed, answer) {
  return { over: listed, answer };
}

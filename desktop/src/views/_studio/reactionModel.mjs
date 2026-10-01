/**
 * Reactions on one message, folded for display.
 *
 * A plain `.mjs` so `node --test` can reach it without a DOM, following the
 * rule the desktop README states: logic a wrong answer would make a wrong
 * *fact* goes beside its component and gets tested. This became that the moment
 * agents started reacting — a reaction used to be a person clicking, where a
 * miscount is cosmetic, and is now the record of which agent picked your
 * message up.
 */

/** The emoji offered in the hover picker. */
export const QUICK_EMOJI = ["👍", "🎉", "👀", "🙏", "❤️"];

/** The mark an agent puts on a message it has taken. Mirrors `TAKEN_MARK` in
 *  `bisa-engine/src/conversation.rs`. */
export const TAKEN_MARK = "👀";

/**
 * One entry per emoji on `targetId`, in first-reacted order.
 *
 * Retracted reactions are dropped rather than shown struck through: a
 * retraction means the person took it back, and a reaction nobody stands behind
 * is not a fact about the message.
 *
 * `mine` carries **this viewer's reaction event id**, not a boolean, because it
 * is also the retract target — so the pill both knows it is pressed and knows
 * what to undo. It is set only for the viewer's own, which is why clicking a
 * pill an agent drew adds your own rather than removing theirs.
 */
export function groupReactions(reactions, targetId, me) {
  const groups = new Map();
  for (const r of reactions) {
    if (r.retracted || r.target_id !== targetId) continue;
    const g = groups.get(r.emoji) ?? { emoji: r.emoji, count: 0, authors: [] };
    g.count += 1;
    g.authors.push(r.author);
    if (r.author === me) g.mine = r.id;
    groups.set(r.emoji, g);
  }
  return [...groups.values()];
}

/**
 * What a composer's box holds once a send settled, and what is kept of a
 * draft while one is on its way. Out of the component so a test can reach
 * it.
 *
 * The box stays open while a message is being sent, and a person goes on
 * typing in it. So a send clears **what it sent** and nothing typed since:
 * emptied whole when the send settled, the next line went with it. And the
 * stored draft is dropped at the press, not at the answer — a person who
 * sends and leaves the room at once must not find the message they sent
 * waiting in the box as a draft, one Enter from being said twice. A send
 * that fails puts it back. Plain `.mjs`, so `node --test` reads it.
 */

/** Where a scope's draft is kept. @param {string} scope */
export function draftKey(scope) {
  return `${DRAFT_PREFIX}${scope}`;
}

/** The prefix every draft's key starts with. */
export const DRAFT_PREFIX = "bisa:draft:";

/**
 * The box after a send that landed: what was typed since, and none of what
 * was sent.
 * @param {string} current what the box holds now
 * @param {string} sent what the box held when the send was pressed
 * @returns {string}
 */
export function afterSend(current, sent) {
  if (current === sent) return "";
  if (sent !== "" && current.startsWith(sent)) return current.slice(sent.length).replace(/^\s+/, "");
  return current;
}

/**
 * The draft to keep for a scope while a send is on its way: none — the
 * words are on their way, and are a message or a failure, never a draft.
 * @returns {null}
 */
export function draftWhileSending() {
  return null;
}

/**
 * The draft to keep after a send that failed: what the box holds now, or —
 * when the box is gone or was emptied — the words that were not sent.
 * @param {string | null | undefined} current what the box holds; `null` when there is no box any more
 * @param {string} sent
 * @returns {string | null}
 */
export function draftAfterFailure(current, sent) {
  const kept = current && current.trim() !== "" ? current : sent;
  return kept === "" ? null : kept;
}

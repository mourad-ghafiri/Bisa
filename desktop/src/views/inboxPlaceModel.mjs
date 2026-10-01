/**
 * Where the Inbox keeps what it remembers of one row (`Inbox.tsx`): the
 * notices a person opened under *What happened*, whether they were all
 * shown, where the row's detail was scrolled. A row is a thing of its own —
 * a goal, a workflow, a project — so what is kept of it is kept under a
 * place of its own, beneath the Inbox's: the memory forgets the rows a
 * person has not been on for longest (`shell/keptMemoryModel`), and
 * forgetting the Inbox's place forgets every row's with it. Facts only, no
 * React. Plain `.mjs`, so `node --test` reads it.
 */

/** The longest key a row's place is made of; a longer one is cut, never refused. */
const MAX_KEY = 256;

/**
 * The place one row's memory is kept under: beneath the Inbox's, by the
 * row's key. No row is the Inbox's own place.
 * @param {string} inbox the Inbox's place
 * @param {string | null | undefined} key the row's key
 * @returns {string}
 */
export function rowPlace(inbox, key) {
  if (typeof key !== "string" || key.length === 0) return inbox;
  return `${inbox}/${key.slice(0, MAX_KEY)}`;
}

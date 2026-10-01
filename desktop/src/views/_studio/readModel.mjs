/**
 * When a conversation reads itself (13-conversations, the guide's Inbox
 * chapter): the surface that shows a thread is the surface that marks it
 * read — the IDE's Agent pane, a goal's or a workflow's conversation, a
 * channel, a direct message, the Inbox's own pane, a hosted channel, all
 * through one component — and it does so when the person can see the newest
 * message: the window in front, the thread at its bottom, the first page
 * landed, and a beat gone by, so a row passed on the way reads nothing. A
 * row can stand unread with no unread message — a gate or a notice moved
 * its watermark — so what is read is the row's word and the count's alike.
 * Plain `.mjs`, so `node --test` reads it.
 */

/** The beat a shown thread waits before it reads itself — a `j` past a row reads nothing. */
export const READ_AFTER_MS = 800;

/** How far from the bottom, in pixels, still counts as reading the newest. */
export const NEAR_BOTTOM_PX = 80;

/**
 * Whether the scope has anything to read: unread messages, or an inbox row
 * that stands unread for a gate or a notice alone.
 * @param {number | null | undefined} unreadCount
 * @param {{read: boolean} | null | undefined} row the scope's inbox row, when it has one
 */
/**
 * The first unread message's id — where the *new* marker is drawn — from the
 * unread count as the person entered: none when nothing is unread, and none
 * when the whole window is unread, since the boundary is then off the top of
 * what was loaded and a marker at the first row would claim otherwise.
 * @param {readonly {id: string}[]} messages oldest first
 * @param {number} unread
 */
export function firstUnreadId(messages, unread) {
  const n = Math.floor(Number(unread) || 0);
  if (n <= 0 || n >= messages.length) return null;
  return messages[messages.length - n]?.id ?? null;
}

export function readWanted(unreadCount, row) {
  return (Number(unreadCount) || 0) > 0 || (row ? !row.read : false);
}

/**
 * Whether the person can see the newest message: the window is visible, the
 * thread stands at its bottom, and the first page has landed — an empty
 * viewport during the first fetch shows nothing.
 * @param {{visible: boolean, atBottom: boolean, loaded: boolean}} facts
 */
export function shownToReader(facts) {
  return Boolean(facts?.visible && facts?.atBottom && facts?.loaded);
}

/**
 * Whether a viewport stands at its bottom, within the tolerance.
 * @param {number} scrollHeight @param {number} scrollTop @param {number} clientHeight
 */
export function atBottomOf(scrollHeight, scrollTop, clientHeight) {
  return Number(scrollHeight) - Number(scrollTop) - Number(clientHeight) < NEAR_BOTTOM_PX;
}

/**
 * The inbox rows with one marked read — what the shell shows the moment a
 * read is asked, before the node's delta confirms it; the same array when
 * the key is not there or the row is read already.
 * @template {{key: string, read: boolean, unread_count: number, unread_notices: number}} R
 * @param {readonly R[]} rows @param {string} key
 * @returns {readonly R[]}
 */
export function rowRead(rows, key) {
  const at = rows.findIndex((r) => r.key === key);
  if (at < 0 || rows[at].read) return rows;
  const next = rows.slice();
  next[at] = { ...rows[at], read: true, unread_count: 0, unread_notices: 0 };
  return next;
}

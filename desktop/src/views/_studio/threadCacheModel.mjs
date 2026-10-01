/**
 * The messages a thread had, for the life of the window (13 —
 * Conversations): a thread that comes back draws what it held at once — the
 * older pages a person had read back to included — and reads its newest page
 * behind that, so the message they were on is still in the page and nothing
 * stands empty until the node answers. Memory only: a message is the node's
 * word, and after a restart the node is read.
 *
 * The newest page **joins** what is held (`joinNewest`, as the Pulse's
 * `joinHead` does for its feed): inside the page's span the page is the
 * truth — a retracted message replaces its row, a row the page no longer has
 * is gone — and what is older, or landed after the page was read, is kept. A
 * full page that reaches nothing held means more landed than a page holds:
 * joining would leave a hole nothing ever fills, so the thread starts over
 * from the page.
 *
 * Bounded twice: the newest `MAX_THREADS` threads, the newest `MAX_MESSAGES`
 * of each. Facts only, no React (`threadsStore.ts` keeps them). Plain
 * `.mjs`, so `node --test` reads it.
 */

/** How many threads one window keeps — the newest shown kept. */
export const MAX_THREADS = 24;
/** How many messages of a thread are kept — the newest; what is older is paged in again. */
export const MAX_MESSAGES = 300;

/**
 * The key a thread is kept under: the chat's own key, and the host for a
 * thread read on a workspace this node is a guest of — two hosts may name a
 * scope alike.
 * @param {string} kind
 * @param {string} scope
 * @param {string | null | undefined} [host]
 */
export function threadKey(kind, scope, host) {
  return host ? `${host}/${kind}:${scope}` : `${kind}:${scope}`;
}

/** Oldest first; two stamped in one second keep the order they were given in. */
function byMoment(messages) {
  return messages.sort((a, b) => a.created_at - b.created_at);
}

/**
 * A page laid into what is shown: inside the page's span the page is the
 * truth, and what is older than its oldest row, or no older than its newest,
 * is kept.
 */
function spliced(shown, page) {
  if (page.length === 0) return [...shown];
  const landed = new Set(page.map((m) => m.id));
  const oldest = page[0].created_at;
  const newest = page[page.length - 1].created_at;
  const older = shown.filter((m) => !landed.has(m.id) && m.created_at <= oldest);
  const newer = shown.filter((m) => !landed.has(m.id) && m.created_at > oldest && m.created_at >= newest);
  return byMoment([...older, ...page, ...newer]);
}

/** The kind of event that is a message — `bisa_core::kind::KIND_MESSAGE`; the one kind a frame names a row for. */
const KIND_MESSAGE = 3407;

/**
 * What a nudge reads (13 — Conversations §The reply streams):
 *
 * - `one` — the message a frame names: a reply landing is one row, not a
 *   page of sixty. Each such read stands on its own — a reply longer than
 *   one message lands as several frames in a row, and the read of the second
 *   must never cancel the read of the first;
 * - `page` — the newest page: a frame that names no message (a snapshot), one
 *   whose event is no message (a reaction, a retraction — the page says what
 *   they did to it), a hosted scope, or no frame at all (the bus came back,
 *   or a one-message read failed and the page is how it catches up).
 * @param {{kind?: number, event_id?: string, snapshot?: boolean} | null | undefined} frame
 * @param {boolean} hosted the scope lives on another node
 * @returns {{read: "one", id: string} | {read: "page"}}
 */
export function nudgeRead(frame, hosted) {
  if (frame && !hosted && !frame.snapshot && frame.kind === KIND_MESSAGE && typeof frame.event_id === "string" && frame.event_id) return { read: "one", id: frame.event_id };
  return { read: "page" };
}

/**
 * What a thread holds after a fresh newest page.
 *
 * - Nothing shown yet: the page is the thread.
 * - A short page is the whole thread: it replaces what was shown.
 * - The page reaches what is shown — an id in both: inside its span the page
 *   replaces, and what is older than its oldest row, or newer than its
 *   newest, is kept. What the thread knew of its start stays.
 * - A full page that reaches nothing shown: a gap. The thread starts over
 *   from the page, and older rows are paged in again.
 * @template {{id: string, created_at: number}} M
 * @param {{shown: readonly M[], page: readonly M[], pageSize: number, hasOlder?: boolean}} facts `hasOlder` is what the shown thread knew
 * @returns {{messages: M[], hasOlder: boolean, restarted: boolean}}
 */
export function joinNewest({ shown, page, pageSize, hasOlder = false }) {
  const full = page.length >= pageSize;
  if (shown.length === 0) return { messages: [...page], hasOlder: full, restarted: false };
  if (!full) return { messages: [...page], hasOlder: false, restarted: false };
  const landed = new Set(page.map((m) => m.id));
  if (!shown.some((m) => landed.has(m.id))) return { messages: [...page], hasOlder: true, restarted: true };
  return { messages: spliced(shown, page), hasOlder, restarted: false };
}

/**
 * What a thread holds after a page of it was read again where it stands —
 * the page that holds a message above the newest page, after a mark or a
 * retraction on it: the page replaces its span, both sides are kept, and
 * what the thread knew of its start stays.
 * @template {{id: string, created_at: number}} M
 * @param {{shown: readonly M[], page: readonly M[]}} facts
 * @returns {M[]}
 */
export function joinAround({ shown, page }) {
  return spliced(shown, page);
}

/**
 * The reactions a thread holds after a fresh newest page: the page's, and
 * those on the messages kept from before — a mark on a message the page does
 * not reach is not the page's to take away.
 * @template {{id: string, target_id: string}} R
 * @param {{shown: readonly R[], page: readonly R[], messages: readonly {id: string}[], landed: readonly {id: string}[]}} facts `messages` is the joined thread, `landed` the page's messages
 * @returns {R[]}
 */
export function joinReactions({ shown, page, messages, landed }) {
  const inPage = new Set(landed.map((m) => m.id));
  const held = new Set(messages.map((m) => m.id));
  const seen = new Set(page.map((r) => r.id));
  const kept = shown.filter((r) => !seen.has(r.id) && !inPage.has(r.target_id) && held.has(r.target_id));
  return [...kept, ...page];
}

/**
 * The newest `cap` messages — the same list when it is within the cap.
 * @template M
 * @param {readonly M[]} messages oldest first
 * @param {number} [cap]
 * @returns {readonly M[]}
 */
export function trimmed(messages, cap = MAX_MESSAGES) {
  return messages.length > cap ? messages.slice(-cap) : messages;
}

/**
 * A thread as it is kept: the newest `cap` messages, the reactions on them,
 * and — once anything was cut — more above to page in. A thread within the
 * cap is kept as it stands, by identity.
 * @template {{id: string}} M
 * @template {{target_id: string}} R
 * @param {{messages: readonly M[], reactions: readonly R[], hasOlder: boolean}} thread
 * @param {number} [cap]
 */
export function keptThread(thread, cap = MAX_MESSAGES) {
  const messages = trimmed(thread.messages, cap);
  if (messages === thread.messages) return thread;
  const held = new Set(messages.map((m) => m.id));
  return { messages, reactions: thread.reactions.filter((r) => held.has(r.target_id)), hasOlder: true };
}

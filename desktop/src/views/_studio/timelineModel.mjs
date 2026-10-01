/**
 * How a timeline is laid out from a flat page of messages (05 — Channels):
 * roots in time order each carrying one level of replies, and which
 * consecutive messages read as one turn. Out of `Chat.tsx` so the rules are
 * a test's to reach.
 */

import { dayKey } from "../../i18n/format.mjs";
import { has, tx } from "../../i18n/l10n.mjs";

/** Consecutive messages from one author inside this gap read as one turn. */
export const GROUP_WINDOW_SECONDS = 300;

/**
 * What a row shows for its words. A post the platform authored — the
 * Workflow Agent's note in a goal's thread, the reply-cut note — carries the
 * message of the catalog behind its English (`MessageRow.said`, 17 —
 * Internationalisation), and is said here in the reader's language; every
 * other post, and one whose message this catalog lacks, shows its `content`
 * — a sentence, never an id. The reply-to strip reads the same.
 * @param {{content?: string | null, said?: {id?: unknown, args?: Record<string, unknown> | null} | null} | null | undefined} message
 */
export function contentOf(message) {
  const said = message?.said;
  if (said && typeof said === "object" && typeof said.id === "string" && has(said.id)) return tx({ id: said.id, args: said.args ?? undefined });
  return String(message?.content ?? "");
}

/**
 * Roots in time order, each with its replies — one level: a reply to a
 * reply hangs under the root. A reply whose parent is not on the page is a
 * root of its own, so nothing on the page is hidden.
 * @template {{id: string, reply_to?: string | null}} M
 * @param {readonly M[]} messages oldest first
 * @returns {{root: M, replies: M[]}[]}
 */
export function threads(messages) {
  const all = messages ?? [];
  const byId = new Map(all.map((m) => [m.id, m]));
  const out = [];
  const index = new Map();
  for (const m of all) {
    const parentId = m.reply_to && byId.has(m.reply_to) ? m.reply_to : null;
    if (parentId) {
      const parent = index.get(parentId);
      if (parent) {
        parent.replies.push(m);
        continue;
      }
    }
    const t = { root: m, replies: [] };
    index.set(m.id, t);
    out.push(t);
  }
  return out;
}

/**
 * Whether `m` is drawn compact — under the previous message with no head of
 * its own: the same author, within the window, on the same day, the
 * previous one still standing, and never the first unread, which is where
 * the reader's eye lands.
 * @param {{author: string, created_at: number, id: string}} m
 * @param {{author: string, created_at: number, retracted?: boolean} | undefined} previous
 * @param {string | null | undefined} firstUnreadId
 */
export function isCompact(m, previous, firstUnreadId) {
  return (
    !!previous &&
    previous.author === m.author &&
    !previous.retracted &&
    dayKey(previous.created_at) === dayKey(m.created_at) &&
    m.id !== firstUnreadId &&
    m.created_at - previous.created_at < GROUP_WINDOW_SECONDS
  );
}

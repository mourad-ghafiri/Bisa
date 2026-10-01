/**
 * The Channels and Direct-messages lists' arithmetic, with no React and no
 * DOM in it — the sidebar's sections and the two pages (`Channels.tsx`,
 * `Messages.tsx`) read the same rules, so a row cannot stand in one order
 * here and another there.
 *
 * Two lists, two orders. **Channels** stand still: `general` first, then in
 * creation order — a sidebar that reshuffled its rooms on every message
 * would be hostile, and a channel is found by its name. **Direct channels**
 * are a recency list, the one that moved last first, as every list of
 * conversations people know. What a row says of its last words is the
 * node's `latest` (`GET /channels`, `GET /dms`: who, the first words, when),
 * kept live between loads from the `latest` each `conversation` frame of
 * the bus carries (`withLatest`).
 */

import { t } from "../../i18n/l10n.mjs";
import { filterByTagsAndWords, matchesWords, searchNeedle } from "../../ui/tagSearchModel.mjs";

/**
 * The principals a channel is encrypted to — a direct channel's
 * participants; a workspace-wide channel has none listed. The model's copy
 * of `types.audiencePrincipals`, which is TypeScript and out of a test's reach.
 * @param {{audience?: {audience: string, principals?: readonly string[]}} | null | undefined} channel
 * @returns {string[]}
 */
export function participantsOf(channel) {
  const a = channel?.audience;
  return a && a.audience === "restricted" ? [...(a.principals ?? [])] : [];
}

/**
 * When a room last moved: its latest live post, else its birth.
 * @param {{channel: {created_at: number}, latest?: {at: number} | null} | null | undefined} entry
 */
export function activityOf(entry) {
  return entry?.latest?.at ?? entry?.channel?.created_at ?? 0;
}

/**
 * Channels: `general` first, then in creation order — a stable list a
 * message never reshuffles. The node lists them so already; the rule is
 * stated here so the sidebar and the page cannot drift from it.
 * @template {{channel: {id: string, created_at: number}}} T
 * @param {readonly T[] | null | undefined} entries
 * @returns {T[]}
 */
export function sortChannels(entries) {
  return [...(entries ?? [])].sort((a, b) => {
    const general = Number(b.channel.id === "general") - Number(a.channel.id === "general");
    if (general !== 0) return general;
    return a.channel.created_at - b.channel.created_at || a.channel.id.localeCompare(b.channel.id);
  });
}

/**
 * Direct channels: the one that moved last first; a room nobody wrote in
 * yet stands by its birth, the newest first.
 * @template {{channel: {id: string, created_at: number}, latest?: {at: number} | null}} T
 * @param {readonly T[] | null | undefined} entries
 * @returns {T[]}
 */
export function sortDms(entries) {
  return [...(entries ?? [])].sort(
    (a, b) => activityOf(b) - activityOf(a) || b.channel.created_at - a.channel.created_at || a.channel.id.localeCompare(b.channel.id),
  );
}

/**
 * The channels a search and a tag filter leave: the words are looked for in
 * the name, the topic and the tags — the same search every list of the app
 * runs (`ui/tagSearchModel`).
 * @template {{channel: {name: string, topic?: string | null, tags?: readonly string[] | null}}} T
 * @param {readonly T[] | null | undefined} entries
 * @param {string | null | undefined} query
 * @param {{selected: readonly string[], match: "any" | "every"}} tags
 * @returns {T[]}
 */
export function filterChannels(entries, query, tags) {
  return filterByTagsAndWords(
    entries ?? [],
    tags,
    query ?? "",
    (e) => e.channel.tags ?? [],
    (e) => [e.channel.name, e.channel.topic ?? "", ...(e.channel.tags ?? [])],
  );
}

/**
 * The other people in a direct channel — everyone in it but you.
 * @param {{audience?: {audience: string, principals?: readonly string[]}} | null | undefined} channel
 * @param {string} me your pubkey
 */
export function dmOthers(channel, me) {
  return participantsOf(channel).filter((p) => p !== me);
}

/**
 * A direct channel's label: the others' names, joined — never the stored
 * name, which is hex prefixes — and *Just you* when there is nobody else.
 * @param {{audience?: {audience: string, principals?: readonly string[]}} | null | undefined} channel
 * @param {string} me
 * @param {(pubkey: string) => string} nameOf
 */
export function dmLabel(channel, me, nameOf) {
  const others = dmOthers(channel, me);
  return others.length ? others.map((p) => nameOf(p)).join(", ") : t("shell-omnibox-just");
}

/**
 * Whose face leads a direct channel's row: the first other participant —
 * the agent, in a room with one — else the room itself.
 * @param {{id: string, audience?: {audience: string, principals?: readonly string[]}}} channel
 * @param {string} me
 */
export function dmHead(channel, me) {
  return dmOthers(channel, me)[0] ?? channel.id;
}

/**
 * The direct channels a search leaves: the participants' names are looked
 * for, since that is what a person knows a conversation by.
 * @template {{channel: {audience?: {audience: string, principals?: readonly string[]}}}} T
 * @param {readonly T[] | null | undefined} entries
 * @param {string | null | undefined} query
 * @param {string} me
 * @param {(pubkey: string) => string} nameOf
 * @returns {T[]}
 */
export function filterDms(entries, query, me, nameOf) {
  const needle = searchNeedle(query ?? "");
  return (entries ?? []).filter((e) => matchesWords(needle, dmOthers(e.channel, me).map((p) => nameOf(p))));
}

/**
 * The line under a row: who said the last words, and the words — `null`
 * when nothing was said yet, for the row to say so in its own way.
 * @param {{author: string, snippet: string} | null | undefined} latest
 * @param {(pubkey: string) => string} nameOf
 */
export function latestWords(latest, nameOf) {
  if (!latest) return null;
  return t("screens-channels-latest-line", { author: nameOf(latest.author), snippet: latest.snippet });
}

/**
 * A `conversation` frame's `latest` put on the row it names, as it came. The
 * node says what now stands as a room's last words by its own rule — a post
 * takes the place, a retraction gives it back to the post before, a reaction
 * or a membership event leaves it — so nothing of that rule is mirrored
 * here and nothing is read again: `null` is a room where nothing stands, and
 * the row says so. A snapshot frame (a definition moved) carries no
 * `latest` and changes nothing; neither does a frame about a room this list
 * does not hold, nor one whose `latest` is what the row says already — the
 * same array, so nothing repaints and no conversation is moved for it.
 * @template {{channel: {id: string}, latest?: {author: string, snippet: string, at: number} | null}} T
 * @param {T[]} entries
 * @param {{scope?: string, snapshot?: boolean, latest?: {author: string, snippet: string, at: number} | null} | null | undefined} frame
 * @returns {T[]}
 */
export function withLatest(entries, frame) {
  if (!frame?.scope || frame.snapshot || frame.latest === undefined) return entries;
  const latest = frame.latest;
  let changed = false;
  const next = (entries ?? []).map((e) => {
    if (e.channel.id !== frame.scope || sameWords(e.latest ?? null, latest)) return e;
    changed = true;
    return { ...e, latest };
  });
  return changed ? next : entries;
}

/** Two rooms' last words are the same words: nobody's both, or one author, one snippet, one time. */
function sameWords(a, b) {
  if (a === null || b === null) return a === b;
  return a.author === b.author && a.snippet === b.snippet && a.at === b.at;
}

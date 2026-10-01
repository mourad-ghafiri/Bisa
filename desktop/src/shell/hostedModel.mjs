/**
 * What this node holds of the workspaces it is a guest of (14-collaboration),
 * as facts for the sidebar's *Hosted by …* sections and the hosted screens:
 * the sections in order, each host's words, a hosted message in the shape
 * the chat draws, and the reactions folded the way the store's are.
 */

import { t } from "../i18n/l10n.mjs";
import { roleLabel } from "../views/_settings/peopleModel.mjs";

/**
 * A hosted channel as the screens' channel shape: the host's record with the
 * halves serde may have left out filled in — a workspace audience, an empty
 * roster — so one component draws a channel of this node's and one of a
 * host's alike.
 */
export function channelDefOf(channel) {
  return {
    ...channel,
    audience: channel.audience ?? { audience: "workspace" },
    roster: channel.roster ?? { policy: "listed", agents: [], teams: [], humans: [] },
  };
}

/** The host's name as a section title: its card's name, else its key's first letters. */
export function hostName(hosted) {
  const name = hosted?.host?.name ?? "";
  if (name.trim()) return name.trim();
  const key = String(hosted?.host?.pubkey ?? "");
  return key ? `${key.slice(0, 8)}…` : t("shell-hosted-a-workspace");
}

/** The section's title. */
export function sectionTitle(hosted) {
  return t("shell-hosted-hosted-by", { host: hostName(hosted) });
}

/** The membership's state under the title, when there is one to say. */
export function stateWords(hosted) {
  const s = hosted?.state?.state;
  switch (s) {
    case "member":
      return null;
    case "requested":
      return t("shell-hosted-waiting-host");
    case "refused":
      return t("shell-hosted-refused-reason", { reason: hosted.state.reason });
    case "removed":
      return hosted.state.reason ? t("shell-hosted-removed-reason", { reason: hosted.state.reason }) : t("shell-hosted-removed-host");
    case "left":
      return t("shell-hosted-left");
    default:
      return null;
  }
}

/**
 * What a hosted channel's screen draws, from what the shell holds:
 *
 * - `thread` — the channel is listed: the conversation, whatever else is out;
 * - `reading` — the workspace is not read yet, so nothing can be said of the
 *   membership: a skeleton, never *not a member*, which nobody checked;
 * - `unknown-host` — read, and this node is a guest of no such workspace;
 * - `not-member` — a membership that is not a member's (requested, refused,
 *   removed, left): its state is the sentence;
 * - `unreached` — a member, and the channel is not among those the role
 *   reaches there (or it is gone).
 * @param {{ready: boolean, section: {host: object} | null | undefined, channel: object | null | undefined}} held
 * @returns {"thread" | "reading" | "unknown-host" | "not-member" | "unreached"}
 */
export function hostedScreen({ ready, section, channel }) {
  if (section && channel && isMember(section.host)) return "thread";
  if (!ready) return "reading";
  if (!section) return "unknown-host";
  if (!isMember(section.host)) return "not-member";
  return "unreached";
}

/** Whether the section still lists channels: only a member's does. */
export function isMember(hosted) {
  return hosted?.state?.state === "member";
}

/**
 * The sections in order: members first, then the rest, each by name. A host
 * this node left or was removed from keeps a section so the words stand,
 * until the person leaves it for good.
 * @param {readonly { host: object, channels?: object[], dms?: object[] }[]} hosts
 */
export function orderSections(hosts) {
  const all = [...(hosts ?? [])];
  all.sort((a, b) => {
    const ma = isMember(a.host) ? 0 : 1;
    const mb = isMember(b.host) ? 0 : 1;
    return ma - mb || hostName(a.host).localeCompare(hostName(b.host));
  });
  return all;
}

/** Unread across every hosted scope — the sidebar's count for a section. */
export function unreadOf(section) {
  const rows = [...(section?.channels ?? []), ...(section?.dms ?? [])];
  return rows.reduce((n, r) => n + (r.unread_count ?? 0), 0);
}

/** The entry for one scope on one host, if this node lists it. */
function entryOf(section, scope) {
  return [...(section?.channels ?? []), ...(section?.dms ?? [])].find((e) => e.channel?.id === scope) ?? null;
}

/**
 * How many messages are unread in one hosted scope — what a hosted thread
 * reads to know whether it has anything to mark (13-conversations).
 * @param {readonly {host: {host: {pubkey: string}}, channels: readonly {channel: {id: string}, unread_count?: number}[], dms: readonly {channel: {id: string}, unread_count?: number}[]}[]} sections
 * @param {string} host the host's pubkey @param {string} scope
 */
export function hostedUnread(sections, host, scope) {
  const section = (sections ?? []).find((s) => s.host?.host?.pubkey === host);
  return entryOf(section, scope)?.unread_count ?? 0;
}

/**
 * The sections with one hosted scope marked read — what the shell shows the
 * moment a read is asked of the host, since no frame carries a hosted read
 * back; the same array when nothing changes.
 * @template {{host: {host: {pubkey: string}}, channels: readonly {channel: {id: string}, unread_count: number}[], dms: readonly {channel: {id: string}, unread_count: number}[]}} S
 * @param {readonly S[]} sections @param {string} host @param {string} scope
 * @returns {readonly S[]}
 */
export function hostedRead(sections, host, scope) {
  const at = (sections ?? []).findIndex((s) => s.host?.host?.pubkey === host);
  if (at < 0) return sections;
  const section = sections[at];
  const clear = (rows) => rows.map((e) => (e.channel?.id === scope && e.unread_count ? { ...e, unread_count: 0 } : e));
  const channels = clear(section.channels ?? []);
  const dms = clear(section.dms ?? []);
  if (channels.every((e, i) => e === section.channels[i]) && dms.every((e, i) => e === section.dms[i])) return sections;
  const next = sections.slice();
  next[at] = { ...section, channels, dms };
  return next;
}

/**
 * A hosted message in the row shape the chat draws — the same fields as a
 * message of this node's, so one timeline component serves both. Files are
 * descriptors whose bytes stay on the host, so none is present here.
 * @param {{ id: string, scope: string, author: string, at: number, text: string, reply_to?: string | null, retracted: boolean, attachments?: object[], artifacts?: object[] }} m
 */
function messageRow(m) {
  return {
    id: m.id,
    scope_id: m.scope,
    author: m.author,
    content: m.text ?? "",
    created_at: m.at,
    reply_to: m.reply_to ?? null,
    retracted: !!m.retracted,
    attachments: (m.attachments ?? []).map((a) => ({ ...a, present: false })),
    artifacts: (m.artifacts ?? []).map((a) => ({ ...a, present: false })),
  };
}

/**
 * A hosted message's reactions in the row shape the fold reads. A hosted
 * reaction has no id of its own on this side, so the row's is made of the
 * message, the author and the emoji — stable, and never sent anywhere.
 * @param {{ id: string, at: number, reactions?: { author: string, emoji: string }[] }} m
 */
function reactionRows(m) {
  return (m.reactions ?? []).map((r) => ({
    id: `${m.id}:${r.author}:${r.emoji}`,
    target: m.id,
    scope_id: m.scope,
    author: r.author,
    emoji: r.emoji,
    created_at: m.at,
  }));
}

/** A page of hosted messages as the chat's page: rows and their reactions. */
export function hostedPage(messages) {
  const rows = (messages ?? []).map(messageRow);
  const reactions = (messages ?? []).flatMap(reactionRows);
  return { messages: rows, reactions };
}

/**
 * The name of someone in a hosted workspace: the host's directory's label,
 * else the key's first letters. The host's agents are not in the directory;
 * their keys read as such until the host's people name them.
 * @param {readonly { pubkey: string, label?: string | null }[]} directory
 */
export function hostedNameOf(directory, pubkey) {
  const hit = (directory ?? []).find((d) => d.pubkey === pubkey);
  if (hit?.label) return hit.label;
  return pubkey ? `${String(pubkey).slice(0, 8)}…` : t("shell-hosted-someone");
}

/**
 * The face of someone in a hosted workspace: the host's directory's photo,
 * by content hash — its bytes came from the host and are served by this
 * node — else none.
 * @param {readonly { pubkey: string, photo?: {sha256: string} | null }[]} directory
 */
export function hostedPhotoOf(directory, pubkey) {
  return (directory ?? []).find((d) => d.pubkey === pubkey)?.photo ?? null;
}

/** Who a hosted composer may address with `@`: the directory, minus you. */
export function hostedMentionables(directory, me) {
  return (directory ?? [])
    .filter((d) => d.pubkey !== me)
    .map((d) => ({ id: d.pubkey, name: hostedNameOf(directory, d.pubkey), kind: "human", description: roleLabel(d.role) }));
}

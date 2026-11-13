/**
 * What the channel dialogs hold and what a save of them writes (05 —
 * Channels), out of the component so a test can reach it.
 *
 * A **draft** is the channel as it stands: its topic, the agent and the
 * people halves of its roster, its tags. A **save** names what the dialog
 * owns — and, editing, only what changed: the node changes the fields a
 * body names on the record as it stands, so a dialog left open while the
 * roster was changed elsewhere saves its topic and leaves the roster as it
 * now is. A topic emptied is sent as `""`, which is how the node hears
 * *clear it* — `null` reads there as a field nobody named.
 *
 * `general` is everyone's room: its roster is a policy, not a list, and the
 * node keeps it whatever is named. So its editor offers the topic and the
 * tags, and a save of it never names a roster. Plain `.mjs`, so
 * `node --test` reads it.
 */

/** One value as the wire would carry it. */
const said = (value) => JSON.stringify(value ?? null);

/** The one channel every workspace has, and nobody deletes. */
const GENERAL = "general";

/** A blank draft: a channel is named first. */
export function emptyChannelDraft() {
  return { name: "", topic: "", agents: [], humans: [], tags: [] };
}

/** The listed halves of a roster; a policy that lists nobody has none. */
function listed(roster) {
  return roster && roster.policy === "listed" ? roster : { agents: [], humans: [] };
}

/**
 * The draft of a channel as it stands.
 * @param {{name: string, topic?: string | null, roster?: object, tags?: readonly string[]}} channel
 */
export function channelDraftOf(channel) {
  const roster = listed(channel.roster);
  return {
    name: channel.name,
    topic: channel.topic ?? "",
    agents: [...(roster.agents ?? [])],
    humans: [...(roster.humans ?? [])],
    tags: [...(channel.tags ?? [])],
  };
}

/**
 * Whether a channel's editor offers a roster: a standing channel other than
 * `general`. A control whose every choice the node throws away is no control.
 * @param {{id: string, kind?: string} | null | undefined} channel
 */
export function editsRoster(channel) {
  return !!channel && channel.kind === "standing" && channel.id !== GENERAL;
}

/**
 * Whether a channel may be edited at all: a standing one. A direct
 * channel's audience is who its past messages were encrypted to.
 * @param {{kind?: string} | null | undefined} channel
 */
export function mayEditChannel(channel) {
  return !!channel && channel.kind === "standing";
}

/**
 * Whether a channel may be deleted: any channel but `general` — a standing
 * one and a direct one alike.
 * @param {{id: string} | null | undefined} channel
 */
export function mayDeleteChannel(channel) {
  return !!channel && channel.id !== GENERAL;
}

/** Whether a new channel may be made: a name, and no write on its way. @param {{name: string}} d @param {boolean} busy */
export function mayCreateChannel(d, busy) {
  return !busy && d.name.trim() !== "";
}

/** The body of a new channel. @param {ReturnType<typeof emptyChannelDraft>} d */
export function channelCreateOf(d) {
  const topic = d.topic.trim();
  return { name: d.name.trim(), ...(topic ? { topic } : {}), agents: d.agents, humans: d.humans, tags: d.tags };
}

/**
 * What a save of an edit sends: the fields that changed since the dialog
 * opened, and no other. An edit that changed nothing sends nothing.
 * @param {ReturnType<typeof emptyChannelDraft>} d the draft
 * @param {{id: string, kind?: string}} channel the channel edited
 * @param {ReturnType<typeof emptyChannelDraft>} seeded the draft as the dialog opened
 * @returns {{topic?: string, agents?: string[], humans?: string[], tags?: string[]}}
 */
export function channelSaveOf(d, channel, seeded) {
  const body = {};
  if (d.topic.trim() !== seeded.topic.trim()) body.topic = d.topic.trim();
  if (editsRoster(channel)) {
    if (said(d.agents) !== said(seeded.agents)) body.agents = d.agents;
    if (said(d.humans) !== said(seeded.humans)) body.humans = d.humans;
  }
  if (said(d.tags) !== said(seeded.tags)) body.tags = d.tags;
  return body;
}

/**
 * The channel a page draws: the list's copy when the list holds it — the
 * list follows every write of the record, here or anywhere — else what the
 * page itself read.
 * @template T
 * @param {readonly {channel: T & {id: string}}[] | null | undefined} entries the workspace's list
 * @param {string | null | undefined} id
 * @param {T | null | undefined} read what the page read
 * @returns {T | null}
 */
export function shownChannel(entries, id, read) {
  const held = (entries ?? []).find((entry) => entry.channel.id === id);
  return held ? held.channel : (read ?? null);
}

/**
 * A word that changes when a channel's record does — what a page's own
 * read is asked again on, so what it shows beside the record (who a handle
 * reaches) follows the record.
 * @param {object | null | undefined} channel
 */
export function channelStamp(channel) {
  return channel ? said(channel) : "";
}

/**
 * The agent ids a roster names that resolve to no agent here — a
 * definition deleted, or one this node has not synced. The picker cannot
 * show them, so the dialog lists them, each with a way to take it off.
 * @param {readonly string[]} ids @param {readonly {id: string}[]} agents
 */
export function unresolvedRoster(ids, agents) {
  const known = new Set((agents ?? []).map((a) => a.id));
  return (ids ?? []).filter((id) => !known.has(id));
}

/**
 * How many agents a channel's handle reaches, by the node's own answer
 * (`roster_pubkeys`: team members in, disabled agents out, the two core
 * agents last) — `null` until the node has answered.
 * @param {{roster_pubkeys?: readonly string[]} | null | undefined} read
 * @returns {number | null}
 */
export function reachOf(read) {
  return Array.isArray(read?.roster_pubkeys) ? read.roster_pubkeys.length : null;
}

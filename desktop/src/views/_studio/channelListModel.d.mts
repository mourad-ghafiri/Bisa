/**
 * Types for `channelListModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { Audience, ChannelListEntry, ConversationFrame, Representative } from "../../types";
import type { TagFilterState } from "../../ui";

/** What the rules need of a room: its id, its birth, who it is encrypted to. */
export interface RoomLike {
  channel: { id: string; created_at: number; name?: string; topic?: string | null; tags?: readonly string[] | null; audience?: Audience };
  latest?: Representative | null;
}

export declare function participantsOf(channel: { audience?: Audience } | null | undefined): string[];
export declare function activityOf(entry: RoomLike | null | undefined): number;
/** `general` first, then creation order — a message never reshuffles the channels. */
export declare function sortChannels<T extends RoomLike>(entries: readonly T[] | null | undefined): T[];
/** The direct channel that moved last first. */
export declare function sortDms<T extends RoomLike>(entries: readonly T[] | null | undefined): T[];
/** The channels a search and a tag filter leave: name, topic and tags searched. */
export declare function filterChannels<T extends RoomLike>(entries: readonly T[] | null | undefined, query: string | null | undefined, tags: TagFilterState): T[];
export declare function dmOthers(channel: { audience?: Audience } | null | undefined, me: string): string[];
/** The others' names, joined; *Just you* when alone. */
export declare function dmLabel(channel: { audience?: Audience } | null | undefined, me: string, nameOf: (pubkey: string) => string): string;
/** Whose face leads the row. */
export declare function dmHead(channel: { id: string; audience?: Audience }, me: string): string;
/** The direct channels a search leaves: the participants' names searched. */
export declare function filterDms<T extends RoomLike>(entries: readonly T[] | null | undefined, query: string | null | undefined, me: string, nameOf: (pubkey: string) => string): T[];
/** *author: snippet*, or null when nothing was said yet. */
export declare function latestWords(latest: Representative | null | undefined, nameOf: (pubkey: string) => string): string | null;
/** A `conversation` frame's `latest` — the node's word for what now stands as the room's last words, `null` for nothing — put on the row it names; the same array when nothing changes. */
export declare function withLatest<T extends RoomLike>(entries: T[], frame: Pick<ConversationFrame, "scope" | "snapshot" | "latest"> | null | undefined): T[];
/** The entries the shell holds: the node's rows, each with its `latest`. */
export type ListEntry = ChannelListEntry;

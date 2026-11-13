/** Types for `channelDraftModel.mjs`. */
import type { ChannelDef, NewChannelBody, PatchChannelBody } from "../../types";

/** A channel as its dialog holds it. */
export interface ChannelDraft {
  name: string;
  topic: string;
  agents: string[];
  humans: string[];
  tags: string[];
}

export declare function emptyChannelDraft(): ChannelDraft;
export declare function channelDraftOf(channel: Pick<ChannelDef, "name" | "topic" | "roster" | "tags">): ChannelDraft;
export declare function editsRoster(channel: Pick<ChannelDef, "id" | "kind"> | null | undefined): boolean;
export declare function mayEditChannel(channel: Pick<ChannelDef, "kind"> | null | undefined): boolean;
export declare function mayDeleteChannel(channel: Pick<ChannelDef, "id"> | null | undefined): boolean;
export declare function mayCreateChannel(d: Pick<ChannelDraft, "name">, busy: boolean): boolean;
export declare function channelCreateOf(d: ChannelDraft): NewChannelBody;
/** What a save of an edit sends: the fields that changed, and no other. */
export declare function channelSaveOf(d: ChannelDraft, channel: Pick<ChannelDef, "id" | "kind">, seeded: ChannelDraft): PatchChannelBody;
export declare function shownChannel<T extends { id: string }>(entries: readonly { channel: T }[] | null | undefined, id: string | null | undefined, read: T | null | undefined): T | null;
export declare function channelStamp(channel: object | null | undefined): string;
export declare function unresolvedRoster(ids: readonly string[], agents: readonly { id: string }[]): string[];
export declare function reachOf(read: { roster_pubkeys?: readonly string[] } | null | undefined): number | null;

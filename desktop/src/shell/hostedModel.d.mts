import type { Channel, ChannelDef, Directory, Hosted, HostedMessage, MessageRow, ReactionRow } from "../types";
import type { Mentionable } from "../ui";

/** A hosted channel with what this person has not read of it, in the screens' channel shape. */
export interface HostedEntry {
  channel: ChannelDef;
  unread_count: number;
  latest_at?: number | null;
}
export interface HostedSection {
  host: Hosted;
  channels: HostedEntry[];
  dms: HostedEntry[];
}
export declare function channelDefOf(channel: Channel): ChannelDef;
export declare function hostName(hosted: Hosted | null | undefined): string;
export declare function sectionTitle(hosted: Hosted | null | undefined): string;
export declare function stateWords(hosted: Hosted | null | undefined): string | null;
export declare function isMember(hosted: Hosted | null | undefined): boolean;
export declare function orderSections(hosts: readonly HostedSection[] | null | undefined): HostedSection[];
export declare function unreadOf(section: { channels?: readonly { unread_count?: number }[]; dms?: readonly { unread_count?: number }[] } | null | undefined): number;
export declare function hostedUnread(sections: readonly HostedSection[] | null | undefined, host: string, scope: string): number;
export declare function hostedRead(sections: readonly HostedSection[], host: string, scope: string): readonly HostedSection[];
export declare function hostedPage(messages: readonly HostedMessage[] | null | undefined): { messages: MessageRow[]; reactions: ReactionRow[] };
export declare function hostedNameOf(directory: readonly Directory[] | null | undefined, pubkey: string): string;
export declare function hostedPhotoOf(directory: readonly Directory[] | null | undefined, pubkey: string): NonNullable<Directory["photo"]> | null;
export declare function hostedMentionables(directory: readonly Directory[] | null | undefined, me: string): Mentionable[];
/** What a hosted channel's screen draws from what the shell holds — the thread, a skeleton while the workspace is unread, or which of three things is missing. */
export declare function hostedScreen(held: { ready: boolean; section: HostedSection | null | undefined; channel: ChannelDef | null | undefined }): "thread" | "reading" | "unknown-host" | "not-member" | "unreached";

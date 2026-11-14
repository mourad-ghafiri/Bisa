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
  /** The host's people as it last said them — read with the section, and again when the host's side moves; `null` when the read did not answer. */
  directory: Directory[] | null;
}
/** Whether a row of a thread is this person's own word no relay has taken yet. */
export declare function isUnsent(row: unknown): boolean;
/** What is done with a membership: left while it stands or waits, forgotten once it is over. */
export declare function membershipAction(hosted: Hosted | null | undefined): "leave" | "forget";
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
/** Whether an engine fact says this hosted thread moved. */
export declare function hostedMoved(payload: { type?: string; host?: unknown; scope?: unknown } | null | undefined, host: string | null | undefined, scope: string | null | undefined): boolean;
/** The key a scope's own things are kept under in this window: the scope, with its host for a hosted one. */
export declare function scopeKey(scope: string, host: string | null | undefined): string;

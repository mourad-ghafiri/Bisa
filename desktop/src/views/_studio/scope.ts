/**
 * Conversation scopes.
 *
 * One message kind carries every message stream (a channel, a DM, a goal's
 * thread, a conversation), so the only thing that differs per surface is
 * which endpoint reads and writes it. That difference lives here, once.
 */

import { type PageBefore, api } from "../../api";
import type { ArtifactRef, AttachmentRef, ContextRef, ConversationKind, MessageRow, ReactionRow } from "../../types";
import { hostedPage } from "../../shell/hostedModel.mjs";

/** Every message stream kind — a conversation included (13 — Conversations). */
export type ScopeKind = ConversationKind;

/**
 * Where a scope lives: this node, or a workspace this node is a guest of
 * (14-collaboration) — the host's pubkey. A hosted scope reads and writes
 * through `/hosts/{host}/…`, and its messages arrive in the guest's shape,
 * folded into the rows the chat draws by `hostedModel.mjs`.
 */
export type ScopeHome = { host: string } | null;

interface ScopePage {
  messages: MessageRow[];
  reactions: ReactionRow[];
}

/** A DM is a channel with a restricted audience — same endpoints. */
export function readMessages(
  kind: ScopeKind,
  id: string,
  before: PageBefore | undefined,
  limit: number,
  signal?: AbortSignal,
  home: ScopeHome = null,
): Promise<ScopePage> {
  if (home) return api.hostedMessages(home.host, id, before, limit, signal).then((r) => hostedPage(r.messages));
  switch (kind) {
    case "goal":
      return api.goalMessages(id, before, limit, signal);
    case "conversation":
      return api.conversationMessages(id, before, limit, signal);
    case "channel":
    case "dm":
      return api.channelMessages(id, before, limit, signal);
  }
}

export function writeMessage(
  kind: ScopeKind,
  id: string,
  body: {
    content: string;
    reply_to?: string;
    mentions?: string[];
    attachments?: AttachmentRef[];
    /** What the person shares to be looked at (ide/12). */
    artifacts?: ArtifactRef[];
    context?: ContextRef[];
  },
  signal?: AbortSignal,
  home: ScopeHome = null,
): Promise<{ id: string }> {
  if (home) {
    // A guest's post is text, mentions and a parent: the bytes of a file
    // would have nowhere to go, and a chip is a fact about this machine.
    return api.postHostedMessage(home.host, id, { content: body.content, mentions: body.mentions, reply_to: body.reply_to }, signal);
  }
  switch (kind) {
    case "goal":
      return api.postGoalMessage(id, body, signal);
    case "conversation":
      return api.postConversationMessage(id, body, signal);
    case "channel":
    case "dm":
      return api.postChannelMessage(id, body, signal);
  }
}

/**
 * Reactions live in `reactionModel.mjs` so `node --test` can reach the fold
 * without a DOM — it decides which agent marked your message, which is a fact
 * rather than a presentation detail. Re-exported here so call sites keep one
 * import for everything about a conversation scope.
 */
export { QUICK_EMOJI, TAKEN_MARK, groupReactions } from "./reactionModel.mjs";
export type { ReactionGroup } from "./reactionModel.mjs";

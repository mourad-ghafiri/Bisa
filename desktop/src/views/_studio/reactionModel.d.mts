import type { ReactionRow } from "../../types";

export interface ReactionGroup {
  emoji: string;
  count: number;
  /** This viewer's reaction event id, when they reacted — the retract target. */
  mine?: string;
  /** Every pubkey that reacted with this emoji, in the order they did. */
  authors: string[];
}

export declare const QUICK_EMOJI: readonly string[];
export declare const TAKEN_MARK: string;
export declare function groupReactions(
  reactions: ReactionRow[],
  targetId: string,
  me: string,
): ReactionGroup[];

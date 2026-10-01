import type { MessageRow } from "../../types";

export declare const GROUP_WINDOW_SECONDS: number;
/** The row's words: the platform's `said` message in this language when the catalog has it, else the content. */
export declare function contentOf(message: Pick<MessageRow, "content" | "said"> | null | undefined): string;
export declare function threads<M extends { id: string; reply_to?: string | null }>(messages: readonly M[]): { root: M; replies: M[] }[];
export declare function isCompact(
  m: Pick<MessageRow, "author" | "created_at" | "id">,
  previous: Pick<MessageRow, "author" | "created_at" | "retracted"> | undefined,
  firstUnreadId: string | null | undefined,
): boolean;

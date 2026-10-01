/** Types for `threadCacheModel.mjs`. */

export declare const MAX_THREADS: number;
export declare const MAX_MESSAGES: number;

/** A thread as it is held: its messages oldest first, the reactions on them, and whether more stands above. */
export interface HeldThread<M, R> {
  readonly messages: readonly M[];
  readonly reactions: readonly R[];
  readonly hasOlder: boolean;
}

export declare function threadKey(kind: string, scope: string, host?: string | null): string;
/** What a nudge reads: the one message a frame names, or the newest page. */
export declare function nudgeRead(frame: { kind?: number; event_id?: string; snapshot?: boolean } | null | undefined, hosted: boolean): { read: "one"; id: string } | { read: "page" };
export declare function joinNewest<M extends { id: string; created_at: number }>(facts: {
  shown: readonly M[];
  page: readonly M[];
  pageSize: number;
  /** What the shown thread knew of its start. */
  hasOlder?: boolean;
}): { messages: M[]; hasOlder: boolean; restarted: boolean };
export declare function joinAround<M extends { id: string; created_at: number }>(facts: { shown: readonly M[]; page: readonly M[] }): M[];
export declare function joinReactions<R extends { id: string; target_id: string }>(facts: {
  shown: readonly R[];
  page: readonly R[];
  /** The joined thread. */
  messages: readonly { id: string }[];
  /** The page's messages. */
  landed: readonly { id: string }[];
}): R[];
export declare function trimmed<M>(messages: readonly M[], cap?: number): readonly M[];
export declare function keptThread<M extends { id: string }, R extends { target_id: string }>(thread: HeldThread<M, R>, cap?: number): HeldThread<M, R>;

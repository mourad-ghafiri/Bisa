/** Types for `readModel.mjs`, plain JavaScript so `node --test` reads it. */

export declare const READ_AFTER_MS: number;
export declare const NEAR_BOTTOM_PX: number;
export declare function readWanted(unreadCount: number | null | undefined, row: { read: boolean } | null | undefined): boolean;
export declare function shownToReader(facts: { visible: boolean; atBottom: boolean; loaded: boolean }): boolean;
export declare function atBottomOf(scrollHeight: number, scrollTop: number, clientHeight: number): boolean;
export declare function rowRead<R extends { key: string; read: boolean; unread_count: number; unread_notices: number }>(rows: readonly R[], key: string): readonly R[];
export declare function firstUnreadId(messages: readonly { id: string }[], unread: number): string | null;

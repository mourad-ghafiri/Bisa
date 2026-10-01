export declare const BASE_RETRY_MS: number;
export declare const MAX_RETRY_MS: number;
export declare const MALFORMED_WARN_WINDOW_MS: number;
/** The connection as every surface reads it; `lagged` is a pulse, never a rest. */
export type Conn = "connecting" | "open" | "closed" | "lagged";
export type StreamEvent = "attempt" | "opened" | "ended" | "idle";
export declare const FIRST_CONN: Conn;
export declare function connAfter(conn: Conn, event: StreamEvent): Conn;
/** Whether the node is there: open, or the lagged pulse of an open stream. */
export declare function connected(conn: string): boolean;
export declare function endLine(wasOpen: boolean, attempt: number): { level: "info" | "warn" | "debug"; message: string };
export interface BusFilter {
  stream?: string;
  goal?: string;
  scope?: string;
  key?: string;
}
export declare function matches(filter: BusFilter, frame: { stream: string; payload?: { goal?: string; scope?: string; key?: string } | unknown }): boolean;
export declare function parseFrame(data: unknown): { stream: string; payload: unknown } | null;
export declare function retryDelay(attempt: number, jitter?: number): number;
export declare function warnsMalformed(lastWarnedAt: number | null, now: number): boolean;

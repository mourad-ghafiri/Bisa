export declare const DEFAULT_BASE: string;
export declare const API_READ_TIMEOUT_MS: number;
export declare function normalizeBase(base: string): string;
export declare function baseOf(env: string | null | undefined, shell: string | null | undefined): string;
export declare function codeOf(body: unknown): string | null;
export declare function detailOf(body: unknown): Record<string, unknown> | null;
export declare function errorDetail(status: number, statusText: string, body: unknown, text?: string): string;
export declare function isOffline(status: number): boolean;
export declare function isServerError(status: number): boolean;
/** The node answered and said no — a 4xx — as against a malfunction or no answer at all. */
export declare function isRefusal(error: unknown): boolean;
export declare function takesDeadline(method: string): boolean;
/** The shell's tagged loose-file refusal as the node's status and body: a conflict's 409, a missing file's 404, a file over the bound's 413 with `size` and `limit`, else a 400. */
export declare function looseRefusal(e: unknown, untold: string): { status: number; message: string; body: Record<string, unknown> };
export type FailureKind = "aborted" | "timeout" | "unreachable";
/** The id of the catalog message a refusal travels as (`ErrorBody.text.id`), or null. */
export declare function refusalOf(body: unknown): string | null;
/** A deadline in milliseconds as the whole seconds a sentence says: rounded up, never 0. */
export declare function deadlineSeconds(timeoutMs: number): number;
export declare function failureOf(error: unknown, callerAborted: boolean, timedOut?: boolean, timeoutMs?: number): { kind: FailureKind; reason: string; detail?: string | null };
export declare function answerUnreadableWords(): string;
export declare function streamEndedWords(): string;
/** The `Authorization` value for a token — words for the node, never a message. */
export declare function bearer(token: string): string;
export declare function tokenToKeep(answer: unknown): string | null;

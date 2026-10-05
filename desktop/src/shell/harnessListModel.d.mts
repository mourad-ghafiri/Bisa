/** The waits before the harness list is read again, in order. */
export declare const CATALOG_RETRY_MS: readonly number[];
/** Whether a list could not be read, or names no installed harness a person could launch. */
export declare function catalogIncomplete(rows: readonly { installed: boolean; launch: unknown }[] | null): boolean;
/** Milliseconds until the list is read again, or null when it answered or the backoff is spent. */
export declare function nextCatalogRead(rows: readonly { installed: boolean; launch: unknown }[] | null, attempt: number): number | null;

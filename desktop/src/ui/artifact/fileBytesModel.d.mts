/** Types for `fileBytesModel.mjs`. */

/** A read of bytes that threw: over the size the node serves — the size and the limit the node said — or a failure in its own words. */
export type BytesFailure = { state: "too_large"; size: number | null; limit: number | null } | { state: "failed"; error: string };

export declare function bytesFailure(error: unknown): BytesFailure;

/** Types for `identityModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { AttachmentRef } from "../../types";

export declare function ownerRow<M extends { pubkey: string }>(ws: { pubkey: string; members?: readonly M[] } | null | undefined): M | null;
export declare function profileDraft(row: { label?: string | null; photo?: AttachmentRef | null } | null | undefined): { label: string; photo: AttachmentRef | null };
export declare function profileWords(): string;

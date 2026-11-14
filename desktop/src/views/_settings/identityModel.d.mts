/** Types for `identityModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { AttachmentRef } from "../../types";

export declare function ownerRow<M extends { pubkey: string }>(ws: { pubkey: string; members?: readonly M[] } | null | undefined): M | null;
export declare function profileDraft(row: { label?: string | null; photo?: AttachmentRef | null } | null | undefined): { label: string; photo: AttachmentRef | null };
export declare function profileWords(): string;
export interface ProfileDraft {
  label: string;
  photo: AttachmentRef | null;
}
export declare function sameProfile(a: ProfileDraft, b: ProfileDraft): boolean;
/** The form after the owner's row was read again: re-seeded only while untouched. */
export declare function profileSeed(form: { seeded: ProfileDraft; draft: ProfileDraft }, row: { label?: string | null; photo?: AttachmentRef | null } | null | undefined): { seeded: ProfileDraft; draft: ProfileDraft };
export declare function profileKey(row: { label?: string | null; photo?: AttachmentRef | null } | null | undefined): string;

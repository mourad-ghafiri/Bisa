import type { Invite, MemberRole, Permission, PermissionRow, RoleRow } from "../../types";

export declare const HOSTED_ROLES: readonly ["admin", "member", "guest"];
export declare const ROLE_HINT: Record<MemberRole, string>;
export declare function roleLabel(role: string | null | undefined): string;
export declare function permissionLabel(p: string | null | undefined): string;
export declare function matrixRows(
  matrix: { roles: RoleRow[]; permissions: PermissionRow[] } | null | undefined,
): { roles: MemberRole[]; rows: { permission: Permission; label: string; words: string; holds: boolean[] }[] };
export declare function personName(person: { label?: string | null; pubkey: string } | null | undefined): string;
export declare function personWords(person: { role: string; client?: string | null; invited_by?: string | null; channels?: string[] }): string;
export declare function roleChangeWords(from: string, to: string): string | null;
export declare const REMOVE_WORDS: string;
export declare function inviteState(invite: Pick<Invite, "state"> | null | undefined): { word: string; tone: "accent" | "warn" | "ok" | "quiet"; open: boolean };
export declare function orderInvites(invites: readonly Invite[] | null | undefined): Invite[];
export declare function expiryWords(hours: number | null | undefined): string;
export declare function inviteOffer(invite: Pick<Invite, "role" | "channels"> | null | undefined): string;
export declare function admitKey(raw: string, people: readonly { pubkey: string }[], me: string): { key: string; problem: string | null };

export type ParsedInvite = { kind: "link" | "code"; nprofile: string; secret: string } | { kind: "none"; reason: string };
export declare function parseInviteCode(raw: string | null | undefined): ParsedInvite;
export declare function inviteLink(parsed: { nprofile: string; secret: string }): string;
export declare function joinCodeFromUrls(urls: readonly string[] | null | undefined): string | null;

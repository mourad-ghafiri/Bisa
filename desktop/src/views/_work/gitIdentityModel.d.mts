export type IdentitySource = "local" | "global" | "none";
export type IdentityState = "local" | "inherited" | "missing";
export type CommitterReason = "created" | "commit_refused" | "settlement_refused";

export interface Ident {
  name: string;
  email: string;
}

/** The shape of `GET /workstreams/{wid}/git/identity` this module reads. */
export interface GitIdentityLike {
  name?: string | null;
  email?: string | null;
  source: IdentitySource;
  global?: Ident | null;
}

export interface OriginLike {
  origin: string;
}

export declare const IDENTITY_SOURCES: readonly IdentitySource[];
export declare const COMMITTER_REASONS: readonly CommitterReason[];
export declare function identityState(view: GitIdentityLike | null | undefined): IdentityState;
export declare function identityMoved(type: string): boolean;
export declare function canCommit(view: GitIdentityLike | null | undefined): boolean;
export declare function identityBlockedReason(view: GitIdentityLike | null | undefined, where?: string): string | null;
export declare function pinnable(view: GitIdentityLike | null | undefined): boolean;
export declare function suggestionWords(view: (GitIdentityLike & { suggested?: { name: string; email: string; login: string } | null }) | null | undefined): { button: string; sentence: string; ident: { name: string; email: string } } | null;
export declare function committerReasonSentence(reason: CommitterReason | string, slug: string, origin?: OriginLike | null): string;
export declare function identitySentence(view: GitIdentityLike | null | undefined): string;
/** The one click that pins the global pair into the repository, or `null`. */
export declare function pinOffer(view: unknown): { button: string; ident: { name: string; email: string } } | null;

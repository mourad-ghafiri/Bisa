import type { CodeHostCapabilities } from "../../types";

export type CodeHostKindWord = "github" | "gitlab" | "bitbucket";
export type ReviewEventWord = "approve" | "request_changes" | "comment";

export declare const KINDS: readonly CodeHostKindWord[];
export declare const PUBLIC_HOSTS: Readonly<Record<CodeHostKindWord, string>>;
export declare function kindOf(codeHost: string | null | undefined): CodeHostKindWord | null;
export declare function hostLabel(codeHost: string | null | undefined): string;
export declare function prNoun(codeHost: string | null | undefined): "pull request" | "merge request";
export declare function prNounCap(codeHost: string | null | undefined): "Pull request" | "Merge request";
export declare function prNouns(codeHost: string | null | undefined): string;
export declare function cliName(codeHost: string | null | undefined): { program: string; label: string } | null;
export declare function settingsTabFor(codeHost: string | null | undefined): CodeHostKindWord | null;
export declare function reviewEventsOf(caps: Pick<CodeHostCapabilities, "review_events"> | null | undefined): readonly ReviewEventWord[];

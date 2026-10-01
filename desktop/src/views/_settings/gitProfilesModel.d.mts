import type { ForeignInclude, GitProfileView, GitProfilesView, ProfileSpec } from "../../types";

export declare const MAX_LOGIN_LEN: number;
export declare function emptySpec(): ProfileSpec;
export declare function specOf(view: GitProfileView): ProfileSpec;
/** What `PUT /git/profiles/{slug}` takes: the spec's fields by name, the aliases as the field reads them. */
export declare function specBody(spec: ProfileSpec, aliases: string): ProfileSpec;
export declare function validateSpec(spec: ProfileSpec): Record<string, string>;
export declare function slugFor(label: string): string | null;
export declare function aliasesText(aliases: readonly string[] | null | undefined): string;
export declare function parseAliases(text: string | null | undefined): string[];
export declare function profileRow(view: GitProfileView): {
  slug: string;
  title: string;
  where: string;
  who: string;
  key: string | null;
  account: string | null;
  matches: string;
};
export declare function matchesWords(globs: readonly string[] | null | undefined): string;
export declare function versionNote(view: GitProfilesView | null | undefined): string | null;
export declare function foreignWords(include: ForeignInclude): string;
export declare function savedWords(view: GitProfileView): string;
export declare function removedWords(slug: string): string;

/** Types for `markdownHtmlModel.mjs`, plain JavaScript so `node --test` reads it. */

/** DOMPurify's configuration shape for a document; the arrays are frozen at run time and only ever read. */
export interface ProseProfile {
  USE_PROFILES: { html: true };
  ALLOW_DATA_ATTR: false;
  FORBID_TAGS: string[];
  FORBID_ATTR: string[];
}

export declare const TAG_REST: string;
export declare const PROSE_PROFILE: ProseProfile;
export declare function isGfmTaskBox(type: string | null | undefined, disabled: string | null | undefined): boolean;
export declare function withoutFrontMatter(text: string): string;

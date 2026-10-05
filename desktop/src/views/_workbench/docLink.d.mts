/** The document a relative markdown link points at, or `null` when it is not
 * a document inside this scope's tree. */
export declare function resolveDocLink(from: string, href: string): string | null;

/** The heading a same-document link names (`#heading`), decoded and lower-cased; null for a document, a bare `#` or no link. */
export declare function fragmentOf(href: unknown): string | null;

/** Whether a file should render as markdown rather than as source. */
export declare function isMarkdown(path: string): boolean;

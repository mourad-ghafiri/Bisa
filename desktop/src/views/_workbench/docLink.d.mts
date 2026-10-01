/** The document a relative markdown link points at, or `null` when it is not
 * a document inside this scope's tree. */
export declare function resolveDocLink(from: string, href: string): string | null;

/** Whether a file should render as markdown rather than as source. */
export declare function isMarkdown(path: string): boolean;

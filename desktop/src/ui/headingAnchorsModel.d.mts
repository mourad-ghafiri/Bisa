/** GitHub's anchor for one heading's words, unique among the anchors `taken` counts. */
export declare function headingSlug(text: string, taken: Map<string, number>): string;
/** The HTML with each `<h1>`–`<h6>` given its anchor as an `id`; one that has an id, or no words, is left alone. */
export declare function withHeadingIds(html: string): string;

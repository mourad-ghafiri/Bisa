/** A side HEAD holds as text — the diff draws it; `new` and `truncated` carry a note on the bar. */
export type HeadText = { kind: "new" | "truncated" | "ready"; original: string };
export type HeadSides = { kind: "binary" } | HeadText;

export declare function canDiffHead(doc: { scope: string; isFile: boolean; onDisk: boolean }): boolean;
export declare function headSides(sides: { original?: string | null; binary: boolean; truncated: boolean }): HeadSides;
export declare function headDiffWords(kind: HeadSides["kind"]): string;

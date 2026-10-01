export type DocKind = "markdown" | "diagram" | "sheet_text" | "svg" | "html" | "pdf" | "sheet" | "document" | "slides" | "image" | "video" | "audio" | "text";
export type DocMode = "rendered" | "split" | "source";

export declare function docKindOf(path: string | null | undefined): DocKind;
export declare function docModes(kind: DocKind): DocMode[];
export declare function defaultMode(kind: DocKind): DocMode;
export declare function isRenderedDoc(kind: DocKind): boolean;
export declare function renderedLabel(kind: DocKind): string;
/** The word for a mode — the glyph's accessible name and tooltip. */
export declare function modeLabel(kind: DocKind, mode: DocMode): string;
/** The glyph a mode wears, as a name in `ICON`. */
export declare function modeGlyph(mode: DocMode): "rendered" | "splitRight" | "code";
/** Where a file's mode is remembered: the document's own key. */
export declare function docModeKey(scope: string, path: string): string;
export declare function artifactKindOf(kind: DocKind): import("../../types").ArtifactKind;
/** The refusal's words when the node will not serve a file's bytes: its size against the limit the node said. */
export declare function tooLargeWords(size: number | null | undefined, limit: number | null | undefined): string;
export declare function binaryWords(size: number): string;

/** A path as a shell reads it back whole: Terminal.app's backslashes, or single quotes around a control character. */
export declare function shellQuoted(path: string): string;
/** Paths as a terminal types them: each quoted, one space between. */
export declare function typedPaths(paths: readonly string[]): string;
/** Whether a paste must ask the shell what the clipboard holds: it carries files, or no text. */
export declare function pasteAsksShell(paste: { types: Iterable<string> | null | undefined; text: string }): boolean;
export type PasteChoice = { kind: "paths"; paths: string[] } | { kind: "text"; text: string } | { kind: "picture" };
/** What a paste the shell was asked about types: paths, else text, else a saved picture, else the paste's own (empty) text. */
export declare function pasteChoice(holds: { paths?: readonly string[]; image?: boolean } | null | undefined, text: string): PasteChoice;

import type { MentionSpan } from "./mentionModel.mjs";

export declare function wantsFiles(query: string, principalMatches: number): boolean;
export declare function fileSuggestions(query: string, paths: readonly string[], limit?: number): string[];
export declare function insertFileMention(text: string, span: Pick<MentionSpan, "at" | "end">, path: string): { text: string; caret: number };
export declare function fileTokenPresent(text: string, path: string): boolean;
export declare function syncFileMentions(text: string, picked: readonly string[]): { kept: string[]; dropped: string[] };

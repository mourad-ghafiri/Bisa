export declare function slidePaths(names: readonly string[]): string[];
export declare function slideRelsPath(slidePath: string): string;
export declare function notesPath(relsXml: string | null | undefined, slidePath: string): string | null;
export declare function slideText(xml: string): { title: string; paragraphs: string[] };
export declare function slideImages(relsXml: string | null | undefined, slidePath: string): string[];
export declare function resolveEntry(fromPath: string, target: string): string;
export declare function notesText(xml: string | null | undefined): string;
export declare function slideWords(count: number): string;

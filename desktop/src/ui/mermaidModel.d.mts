export declare function mermaidBlocks(markdown: string): { source: string; startLine: number }[];
export declare function offsetError(message: unknown, startLine: number | null | undefined): { line: number | null; message: string };
export declare function mermaidTheme(setting?: unknown): string;
export declare const DIAGRAM_ROLES: readonly string[];
export declare function themeVariablesFor(
  resolved: Readonly<Record<string, string>> | null | undefined,
  scheme: "light" | "dark",
  fontFamily: string,
): Record<string, string | boolean>;
export declare function isMermaidPath(path: string | null | undefined): boolean;
export declare function exportScale(setting?: unknown): number;

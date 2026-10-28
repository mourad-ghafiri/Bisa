/** Types for `editorTheme.mjs`. */

export declare const EDITOR_ROLE_MAP: Readonly<Record<string, readonly string[]>>;

export declare function isHex(value: string): boolean;

/** Monaco's own themes `editor.theme` may name instead of the app's. */
export declare const MONACO_BUILTIN_THEMES: readonly string[];
/** One of Monaco's own themes when `choice` names one, else `appTheme`. */
export declare function monacoThemeFor(choice: unknown, appTheme: string): string;

export interface EditorThemeRule {
  token: string;
  foreground: string;
  fontStyle?: string;
}

export interface EditorThemeData {
  base: "vs" | "vs-dark";
  inherit: boolean;
  rules: EditorThemeRule[];
  colors: Record<string, string>;
}

export declare function editorThemeFor(
  roles: Record<string, string>,
  scheme: "light" | "dark",
): { theme: EditorThemeData; missing: string[] };

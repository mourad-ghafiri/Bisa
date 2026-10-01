/** Types for `editorTheme.mjs`. */

export declare const EDITOR_ROLE_MAP: Readonly<Record<string, readonly string[]>>;

export declare function isHex(value: string): boolean;

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

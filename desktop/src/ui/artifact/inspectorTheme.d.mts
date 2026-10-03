/**
 * Types for `inspectorTheme.mjs`. This file is the only reason TypeScript
 * never has to read it.
 */

export declare const BOX_WIDTH: number;
export declare const INSPECTOR_ROLES: readonly string[];
export declare const FALLBACK_SIZES: Readonly<{ size: number; small: number }>;
export declare const STYLE_PARTS: readonly string[];
/** A colour with its alpha taken off; anything else as it is. */
export declare function opaque(value: string): string;

/** The overlay's theme: the role tokens, resolved, on one side. */
export interface InspectorTheme {
  readonly scheme: "light" | "dark";
  readonly color: Readonly<{
    surface: string;
    surface2: string;
    border: string;
    text: string;
    textDim: string;
    accent: string;
    accentInk: string;
    accentSoft: string;
    accentContrast: string;
  }>;
  readonly radius: Readonly<{ control: string; card: string }>;
  readonly shadow: Readonly<{ raised: string; floating: string }>;
  readonly font: Readonly<{ ui: string; mono: string; size: string; small: string }>;
  readonly motion: Readonly<{ fast: string; ease: string }>;
}

/** The `cssText` of every part of the overlay, by `data-bisa-inspector` value (and `part:hover` / `part:focus`). */
export type InspectorStyles = Readonly<Record<string, string>>;

export declare function safeCssValue(value: unknown): string | null;
export declare function inspectorTheme(resolved: Readonly<Record<string, string>>, scheme: "light" | "dark", sizes?: { size?: number; small?: number }): InspectorTheme;
export declare function inspectorStyles(theme: InspectorTheme): InspectorStyles;

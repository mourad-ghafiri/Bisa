/** Types for `localeModel.mjs`. */

export declare const AVAILABLE: readonly string[];
export declare const DEFAULT_LOCALE: "en";
export declare const LANGUAGE_SETTING: "appearance.language";
export declare const LANGUAGE_CHOICES: readonly string[];
export declare const LOCALE_KEY: "bisa.locale";
export declare function negotiate(requested: readonly string[] | null | undefined): string;
export declare function resolveChoice(choice: unknown, systemLanguages: readonly string[] | null | undefined): string;
export declare function isShipped(raw: unknown): raw is string;
export declare function readStoredLocale(storage: Storage | null | undefined): string | null;
export declare function textDirection(locale: string | null | undefined): "ltr" | "rtl";

import type { InspectorTheme } from "./inspectorTheme.mjs";

export declare const PAGE_SANDBOX: string;
export declare const PAGE_CDNS: readonly string[];
export declare function pageCsp(opts: { libraries: boolean }): string;
export declare function pageDocument(html: string, opts?: { libraries?: boolean; scheme?: "light" | "dark"; inspector?: InspectorTheme | null }): string;

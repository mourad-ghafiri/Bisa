/** Types for `osPasteModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { PasteReport } from "../types";

export type PasteSource = "tree" | "os" | "image" | null;

export declare function pasteDestination(root: string | null, dir: string): string | null;
export declare function pasteWords(report: PasteReport, dir: string): { text: string; tone: "ok" | "warn" | "error" };
export declare function pasteRefusal(): string;
export declare function pasteSource(clip: object | null, held: { files: boolean; image: boolean }): PasteSource;

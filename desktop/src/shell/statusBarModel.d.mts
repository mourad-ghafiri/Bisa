export declare function cpuLabel(pct: number | null | undefined): string;
export declare function memLabel(used: number | null | undefined, total: number | null | undefined): string;
export declare function gpuLabel(gpu: { util_percent: number } | null | undefined): string;
export declare function diskLabel(bytes: number | null | undefined): string;
export declare function caretLabel(status: { line: number; column: number } | null | undefined): string;
export declare function languageLabel(status: { language: string | null } | null | undefined): string;
export declare function countWords(what: "terminals" | "harnesses" | "ports", count: number): string;
export declare function portOwnerWord(kind: string | null | undefined): string;

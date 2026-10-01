export declare const FRAME_MS: number;
export declare const MIN_RATE: number;
export declare const JUMP_CHARS: number;
export declare function pace(step: { shown: number; target: number; dtMs: number; sinceMs?: number; frameMs?: number; done?: boolean }): number;
export declare function reveal(text: string, n: number): string;

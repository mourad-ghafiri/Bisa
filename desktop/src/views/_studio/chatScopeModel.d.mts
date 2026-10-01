/** Types for `chatScopeModel.mjs`, plain JavaScript so `node --test` reads it. */

export declare function chatKey(kind: string, id: string): string;
export declare function attachWords(target: { kind: string; id: string } | null): { enabled: boolean; label: string; hint: string };
export declare function attachedWords(count: number): string;

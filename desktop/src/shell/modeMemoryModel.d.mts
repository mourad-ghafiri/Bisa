/** Types for `modeMemoryModel.mjs`: a mode remembered per key, over a caller's vocabulary. */

export declare const DEFAULT_CAP: number;
export declare function modeIn<M extends string>(value: unknown, offered: readonly M[], fallback: M): M;
export declare function modeFor<M extends string>(remembered: unknown, setting: unknown, offered: readonly M[], fallback: M): M;
export declare function nextMode<M extends string>(mode: unknown, offered: readonly M[], fallback: M): M;
export declare function rememberMode<M extends string>(byKey: Readonly<Record<string, M>>, key: string, mode: M, cap?: number): Readonly<Record<string, M>>;
export declare function parseRememberedModes<M extends string>(raw: unknown, offered: readonly M[]): Record<string, M>;

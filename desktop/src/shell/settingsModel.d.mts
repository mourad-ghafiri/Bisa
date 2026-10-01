export interface ResolvedRow {
  key: string;
  value: unknown;
}

export declare function settingOf<T>(resolved: readonly ResolvedRow[] | null | undefined, key: string, fallback: T): unknown | T;
export declare function choiceOf<T extends string>(
  resolved: readonly ResolvedRow[] | null | undefined,
  key: string,
  allowed: readonly T[],
  fallback: T,
): T;
export declare function numberOf(
  resolved: readonly ResolvedRow[] | null | undefined,
  key: string,
  fallback: number,
  bounds?: { min?: number; max?: number },
): number;
export declare function boolOf(resolved: readonly ResolvedRow[] | null | undefined, key: string, fallback: boolean): boolean;
export declare function stringOf(resolved: readonly ResolvedRow[] | null | undefined, key: string, fallback: string): string;

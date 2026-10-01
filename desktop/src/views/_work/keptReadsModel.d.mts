/** Types for `keptReadsModel.mjs`. */

export declare const MAX_READS: number;
export declare function readKey(what: string, ...of: (string | null | undefined)[]): string | null;

export declare class KeptReads {
  constructor(capacity?: number);
  read(key: string | null | undefined): unknown;
  keep(key: string | null | undefined, answer: unknown): void;
  forget(key: string | null | undefined): void;
  clear(): void;
  readonly size: number;
}

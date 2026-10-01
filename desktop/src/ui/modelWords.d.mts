export interface ModelWords {
  /** The id without its provider prefix, then the effort when the row carries one. */
  short: string;
  /** The id as the harness spells it, then the effort when the row carries one. */
  full: string;
}
export declare function modelWords(model: string | null | undefined, effort?: string | null): ModelWords | null;
export declare function nameWithModel(name: string, model: string | null | undefined, effort?: string | null): string;

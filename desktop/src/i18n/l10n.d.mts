/** Types for `l10n.mjs`. */

/** A sentence as data — `bisa_core::text::Text` on the wire. */
export interface Text {
  id: string;
  args?: Record<string, string | number | boolean> | null;
}

export type Args = Record<string, unknown>;

export declare function install(locale: string, sources: readonly string[]): Error[];
export declare function locale(): string;
export declare function has(id: string): boolean;
export declare function onMissing(listener: (id: string) => void): () => void;
export declare function t(id: string, args?: Args): string;
export declare function attr(id: string, name: string, args?: Args): string;
export declare function tx(text: Text | null | undefined): string;

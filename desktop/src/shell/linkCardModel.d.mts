/** Types for `linkCardModel.mjs`. */

import type { DocResolution, LinkResolution } from "../ui/linkModel.mjs";

export declare const MAX_DEFAULT_ROOTS: number;

/** One verb of a card: what it does (`id`), its words, and what it acts on. */
export type CardVerb =
  | { id: "open"; label: string; primary?: boolean; doc: DocResolution }
  | { id: "open_loose" | "reveal"; label: string; primary?: boolean; absolute: string }
  | { id: "open_here" | "open_machine"; label: string; primary?: boolean; url: string }
  | { id: "copy"; label: string; primary?: boolean; text: string; what: "path" | "url" };

export interface CardSpec {
  title: string;
  subtitle: string | null;
  verbs: CardVerb[];
}

export declare function defaultRoots<S extends string>(route: { name: string; scope?: S; id?: string }, workstreams: readonly { exists: boolean; path?: string | null; workstream: { id: string } }[] | null | undefined): { scope: S | "workstream"; id: string }[];
export declare function rootLabel(ref: { scope: string; id: string }, workstream: { project_name?: string | null; workstream: { name?: string | null } } | null | undefined): string;
export declare function pathCard(hit: { path: string }, res: LinkResolution, words: { reveal: string }): CardSpec;
export declare function urlCard(url: string, can: { embedded: boolean }): CardSpec;
export declare function copiedWords(what: "path" | "url", ok: boolean): string;

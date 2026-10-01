export type HolderWord = "you" | "agents" | "world" | "finished" | "design";

export interface StripStepLike {
  id: string;
  name: string;
  kind: string;
  state: { state: string; [k: string]: unknown };
}

export interface StripLike {
  run?: string | null;
  workflow_name?: string | null;
  steps?: StripStepLike[];
  current?: string[];
  reached?: number;
  total?: number;
  outcome?: string | null;
  cancelled?: boolean;
}

export interface StripRowLike {
  id: string;
  title?: string | null;
  statement?: string;
  holder: string;
  strip?: StripLike;
  last_activity_at: number;
}

export interface Filters {
  holder?: string;
  workflow?: string;
  q?: string;
  /** Show the goals put away as well. */
  archived?: boolean;
}

export interface StepChip extends StripStepLike {
  current: boolean;
  label: string;
}

export declare const HOLDERS: readonly HolderWord[];
export declare const HOLDER_LABEL: Record<HolderWord, string>;
export declare const HOLDER_TONE: Record<HolderWord, string>;
export declare const KIND_LABEL: Record<string, string>;
export declare function finishedTone(strip: StripLike | null | undefined): "ok" | "danger";
/** Where a failed strip failed, in a sentence for the row — `null` while nothing failed. */
export declare function failureWords(strip: { failure?: { step: string; name: string; error?: string | null } | null } | null | undefined): string | null;
export declare function chipsOf(strip: StripLike | null | undefined): StepChip[];
export declare function currentStepOf(
  strip: StripLike | null | undefined,
): (StripStepLike & { more: number }) | null;
export declare function workflowFacets(
  rows: readonly StripRowLike[],
): { name: string; count: number }[];
export declare function matchesFilters(row: StripRowLike, f?: Filters): boolean;
export declare function sortByActivity<T extends StripRowLike>(rows: readonly T[]): T[];
export declare function parseFilters(params: URLSearchParams | null | undefined): Filters;
export declare function serializeFilters(f?: Filters): Record<string, string>;

export declare const HOLDER_FILTER_ALL: "all";

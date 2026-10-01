export interface RailFact {
  readonly text: string;
  readonly title: string;
}

export interface RailFactsStatus {
  readonly base?: string | null;
  readonly ahead_of_base?: number | null;
  readonly behind_base?: number | null;
}

/** The row's facts, in order: ahead of the base, then behind it; never a word for the tree's state. */
export function workstreamFacts(status: RailFactsStatus | null | undefined): readonly RailFact[];

/** The one string the row paints, and the one its tooltip carries. */
export function factsWords(facts: readonly RailFact[]): { readonly text: string; readonly title: string };

import type { IconName } from "../../ui/icons";
import type { ActivityConcept, PulseCursor, PulsePage, PulseSource } from "../../types";
import type { Route } from "../../router";

export type Concept = "all" | ActivityConcept;

export declare const CONCEPTS: readonly Concept[];
export declare const PAGE: number;
export declare const PAGE_AHEAD: number;
export declare const NUDGE_MS: number;

export interface ConceptWords {
  label: string;
  /** A key of `ICON`. */
  /** The glyph's key in `ui/icons.ts`. */
  icon: IconName;
  empty: string;
  hint: string;
}

export interface FeedRowLike {
  seq: number;
  at: number;
}

export type Slot<T extends { key: string; at: number; title?: string }> =
  | { slot: "day"; key: string; at: number }
  | { slot: "row"; item: T; heading?: string };

export declare function dayKey(unixSeconds: number): string;
export declare function conceptWords(concept: string): ConceptWords;
export declare function parseConcept(raw: string | null | undefined): Concept;
export declare const NOT_ACTIVITY: readonly string[];
export declare const FACT_CONCEPT: Readonly<Record<string, ActivityConcept>>;
export declare const HOSTED_FACTS: readonly string[];
/** Where an engine fact is filed: its concept, `false` for one that is no activity, `null` for one this build cannot place. */
export declare function conceptOfFact(payload: { type?: unknown; listener?: unknown; host?: unknown } | null | undefined): ActivityConcept | false | null;
export declare function wantsNudge(concept: Concept, of: string | false | null | undefined): boolean;
export declare function wantsMore(state: { last: number; total: number; next: PulseCursor | null; inFlight: boolean }): boolean;
export declare function mergePage<T extends FeedRowLike>(shown: readonly T[], page: readonly T[], where: "head" | "older"): T[];
export declare function cursorOf(page: Pick<PulsePage, "next">): PulseCursor | null;
export declare function withDividers<T extends { key: string; at: number; title?: string }>(items: readonly T[]): Slot<T>[];
export declare function linkOf(source: PulseSource | null | undefined, channelKind: (id: string) => "channel" | "dm" | null): Route | null;

/** The words of a line an item keeps. */
type LineWords = "key" | "text" | "tone" | "author" | "title" | "icon" | "detail";

export declare function itemOf<L extends { key: string; text: string }>(
  row: FeedRowLike & { concept: string; source: PulseSource },
  line: L,
): Pick<L, Extract<keyof L, LineWords>> & { seq: number; at: number; concept: string; source: PulseSource };

export declare function joinHead<T extends FeedRowLike>(facts: {
  shown: readonly T[];
  page: readonly T[];
  next: PulseCursor | null | undefined;
  tail: PulseCursor | null;
  size?: number;
}): { items: T[]; next: PulseCursor | null; restarted: boolean };

export declare const MAX_PAGES: number;
export declare const HELD_MAX: number;
/** The feed as it is held after its head grew: bounded below what the window draws, its cursor at the last row kept. */
export declare function heldFeed<T extends FeedRowLike>(feed: { items: T[]; next: PulseCursor | null }, shownLast?: number, max?: number): { items: T[]; next: PulseCursor | null };
/** How far a feed is read, as it is kept for a restart; 0 for a feed no deeper than its head page. */
export declare function depthOf(loaded: number, size?: number): number;
/** A depth read back from a screen's memory, or `undefined` for what is no depth. */
export declare function parseDepth(raw: unknown, size?: number): number | undefined;
export declare function wantsDepth(state: { loaded: number; wanted: number; next: PulseCursor | null; inFlight: boolean }): boolean;

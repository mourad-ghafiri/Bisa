/** Types for `browserStatModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { BrowserPlaces } from "./browserPlacesModel.mjs";
import type { BrowserSession } from "./browsersModel.mjs";

export type Dimension = "tabs" | "origins" | "unseen";
export type DimensionIcon = "page" | "layout" | "hidden";

export declare const DIMENSIONS: readonly Dimension[];
export declare function dimensionLabel(dimension: Dimension | string): string;
export declare function dimensionIcon(dimension: Dimension | string): DimensionIcon;
export declare function chosenDimension(remembered: string | null | undefined): Dimension;

export interface StatWords {
  /** Every open tab, as the number on the bar. */
  value: string;
  /** The footer's sentence — the tooltip and the overlay's title. */
  title: string;
  /** The working dot's words while an agent browses, else null. */
  live: string | null;
  /** A host draws a tab right now. */
  pressed: boolean;
}
export declare function statWords(facts: { sessions: readonly BrowserSession[]; busy: readonly string[]; shown: string | null }): StatWords;

export interface Segment {
  key: string;
  label: string;
  percent: number;
  tone: "accent" | "quiet";
}
export declare function visibilityBar(sessions: readonly Pick<BrowserSession, "headless">[]): Segment[];

/** Where a row leads: the pane on a tab, showing one kept out of sight. */
export type Door = { kind: "tab"; key: string } | null;

export interface Row {
  key: string;
  label: string;
  sub: string | null;
  /** The value column: a count, *on screen*, *unseen*, or nothing. */
  value: string;
  /** The row's bar, as a share of the largest; null where the value is no share. */
  percent: number | null;
  door: Door;
  /** The tab the row's ✕ closes, or null for a row that closes nothing. */
  close: string | null;
  /** The row's title — the visibility words of a tab kept out of sight. */
  hint: string | null;
  /** The row holds the tab on screen. */
  current: boolean;
  /** Kept out of sight, or an origin with no tab in sight. */
  dim: boolean;
  /** An agent browses here. */
  busy: boolean;
}
export declare function overlayRows(dimension: Dimension, ctx: { sessions: readonly BrowserSession[]; places: BrowserPlaces; busy: readonly string[]; shown: string | null; agentName?: (id: string) => string | null | undefined }): Row[];
export declare function emptyWords(dimension: Dimension | string, count: number): string;
export declare function footnote(policy: string): string;

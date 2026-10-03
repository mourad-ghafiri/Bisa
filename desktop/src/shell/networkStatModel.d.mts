import type { NetworkStatus } from "../types";
import type { NetworkFacts } from "./networkApi";

export type StatValue = "DOWN" | "VPN" | "UP" | "—";
export type StatTone = "ok" | "danger" | "quiet";
export type Tone = "ok" | "warn" | "neutral" | "quiet" | "danger";

export declare const VALUES: readonly StatValue[];

export interface StatWords {
  value: StatValue;
  tone: StatTone;
  title: string;
}
export declare function statWords(facts: NetworkFacts | null, status: NetworkStatus | null, online: boolean | null): StatWords;
/** The word the bar draws beside the glyph: only VPN; up and down are the dot's tone. */
export declare function barWord(words: { value: string }): "VPN" | null;

export interface OverlaySection {
  key: string;
  title: string;
  tone?: Tone;
  label?: string;
  sentence: string | null;
  rows: { label: string; value: string }[];
}
export declare function overlaySections(facts: NetworkFacts | null, status: NetworkStatus | null, online: boolean | null, where: { desktop: boolean }): OverlaySection[];
export declare function footnote(pollMs: number, readAt: number | null, now?: number): string;

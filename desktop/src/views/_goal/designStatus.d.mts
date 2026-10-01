/**
 * Types for `designStatus.mjs`. This file is the only reason TypeScript never
 * has to read it.
 */

import type { ModelLine } from "../../activityModel.mjs";
import type { DesignStatus, Goal, GuidanceInfo } from "../../types";
import type { EnginePayload } from "../../types.hand";

export type DesignKind = "scheduled" | "working" | "asking" | "proposed" | "stalled" | "failed" | "off" | "manual";
export type DesignTone = "neutral" | "accent" | "warn" | "danger" | "ok" | "quiet";

export interface DesignView {
  kind: DesignKind;
  headline: string;
  hint: string;
  tone: DesignTone;
  /** While the agent is at work: how long, as `12s` · `3m 05s` · `1h 12m`. */
  elapsed: string | null;
  primary: "retry" | "pick" | "review" | "design" | null;
  offerPick: boolean;
  offerRetry: boolean;
  offerAsk: boolean;
  activity: ModelLine | null;
}

export declare const DESIGNING_KINDS: readonly DesignKind[];
/** The word of a kind, for the card's chip. */
export declare function kindWord(kind: DesignKind | string | null | undefined): string;
/** Whether the card's clock ticks: the agent is at work, or about to be. */
export declare function ticks(kind: DesignKind | string | null | undefined): boolean;
export declare function designInProgress(guidance: GuidanceInfo | null | undefined, goal: Goal): boolean;
export declare function designView(
  guidance: GuidanceInfo | null | undefined,
  goal: Goal,
  opts?: { now?: number; paused?: boolean; activity?: ModelLine | null },
): DesignView;
export declare function applyGuidedFrame(
  design: DesignStatus | null,
  payload: EnginePayload,
  at: number,
): DesignStatus | null;
export declare function activityFrom(payload: EnginePayload, prev: ModelLine | null): ModelLine | null;

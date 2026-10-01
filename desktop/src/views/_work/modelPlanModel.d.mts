/** Types for `modelPlanModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { ModelHealthRow, ModelInfo, ModelPlan, ModelChoice, ModelStrategy } from "../../types";

export interface StrategyDef {
  value: ModelStrategy;
  label: string;
  /** What it does to *this* list, in one line. */
  explain: string;
  /** What the order of the list means under this strategy. */
  orderMeans: string;
}

/** One thing a row wears for what the ledger knows; `icon` is a key of the kit's `ICON`. */
export interface HealthChip {
  id: "cooling" | "failures" | "flight";
  tone: "warn" | "quiet" | "accent";
  icon: "waiting" | "warn" | "working";
  words: string;
  tip: string;
}

export declare const DEFAULT_STRATEGY: ModelStrategy;
export declare const STRATEGIES: readonly StrategyDef[];
export declare const MAX_WEIGHT: number;
export declare function strategyOf(plan: { strategy?: unknown } | null | undefined): StrategyDef;
export declare function defaultModelKey(harness: string): string;
export declare function healthOf(rows: readonly ModelHealthRow[] | null | undefined, harness: string, model: string | null): ModelHealthRow | undefined;
export declare function healthChips(row: Pick<ModelHealthRow, "retry_in_secs" | "consecutive_failures" | "in_flight"> | null | undefined): HealthChip[];
export declare function weightWords(weight: unknown): string;
export declare function weightFrom(typed: unknown): number;
export declare function answeredBy<A extends { harness?: string }>(harness: string, answer: A | null | undefined, error?: unknown): A | null;
export declare function addModel(plan: ModelPlan, id: string): { plan: ModelPlan; error: null } | { plan: null; error: string | null };
export declare function moveModel(plan: ModelPlan, from: number, delta: number): ModelPlan;
export declare function patchModel(plan: ModelPlan, at: number, patch: Partial<ModelChoice>): ModelPlan;
export declare function removeModel(plan: ModelPlan, at: number): ModelPlan;
export declare function unusedModels(offered: readonly ModelInfo[] | null | undefined, plan: ModelPlan | null | undefined): ModelInfo[];
export declare function enabledCount(plan: ModelPlan | null | undefined): number;

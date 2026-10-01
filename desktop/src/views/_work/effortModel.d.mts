import type { Effort, EffortChoice, SettingOrigin } from "../../types";

export declare const EFFORTS: readonly Effort[];
export declare const EFFORT_CHOICES: readonly EffortChoice[];
export declare const DEFAULT_EFFORT: Effort;
export declare const EFFORT_SETTING: string;

export declare function effortChoice(value: unknown): EffortChoice | null;
export declare function effortLabel(value: unknown): string;
export declare function clampEffort(level: Effort, available: readonly Effort[]): Effort | null;

/** What `GET /harnesses/{id}/models` says of effort: the levels for any model, and each listed model's own. */
export interface HarnessEfforts {
  efforts?: readonly Effort[];
  models?: readonly { id: string; efforts?: readonly Effort[] }[];
}

export declare function effortsFor(harnessModels: HarnessEfforts | null | undefined, modelId: string | null | undefined): Effort[];
export declare function planEfforts(
  harnessModels: HarnessEfforts | null | undefined,
  plan: { models?: readonly { model: string; enabled?: boolean }[] } | null | undefined,
): Effort[];
export declare function effortsAcross(answers: readonly (HarnessEfforts | null | undefined)[] | null | undefined, modelId: string | null | undefined): Effort[];

export interface EffortOption {
  /** The wire's word, or `""` for nothing chosen. */
  value: EffortChoice | "";
  label: string;
  /** A saved value the model no longer offers: shown, and fitted at launch. */
  kept?: boolean;
}
export interface EffortAsked {
  /** Offer *Inherit* — nothing chosen. */
  inherit?: boolean;
  /** Offer *Auto*, when there are two levels to choose between. */
  auto?: boolean;
  /** The harness answered: nothing available is none, not unknown. */
  known?: boolean;
  /** The value already saved, which stays among the options. */
  current?: unknown;
}
export declare function effortOptions(available: readonly Effort[], asked?: EffortAsked): EffortOption[];
export declare function offersEffort(options: readonly EffortOption[] | null | undefined): boolean;

/** The link of the chain that decided. */
export type EffortSource = "step" | "model" | "plan" | "setting";
export type ResolvedEffort =
  | { kind: "level"; level: Effort; from: EffortSource }
  | { kind: "auto"; fallback: Effort; from: EffortSource; fallbackFrom: EffortSource | "default" };
export declare function resolveEffort(step: unknown, model: unknown, plan: unknown, setting: unknown): ResolvedEffort;

export declare function effortSetting(
  resolved: readonly { key: string; value: unknown; origin?: SettingOrigin }[] | null | undefined,
): { setting: EffortChoice; origin: SettingOrigin };

export interface EffortFacts {
  available?: readonly Effort[];
  known?: boolean;
  /** The layer that holds the setting, for a level the setting decided. */
  origin?: SettingOrigin;
}
export interface EffortWords {
  /** The level that runs, fitted to what the model takes; `null` on a harness that lists none. */
  runs: Effort | null;
  /** A row's short word: the level's label, or *Auto*. */
  label: string;
  /** The sentence under a control. */
  hint: string;
}
export declare function effortWords(resolved: ResolvedEffort, facts?: EffortFacts): EffortWords;

export declare function withEffort<T extends object>(holder: T, effort: unknown): T;

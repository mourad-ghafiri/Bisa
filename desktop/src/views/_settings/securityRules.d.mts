import type { GuardDecision, GuardRule, PolicyProblem, RedactRule, SecurityStatus } from "../../types";

export type LineTone = "ok" | "warn" | "danger" | "quiet";

export declare const KEYS: {
  redactor: { enabled: string; rules: string; builtinsOff: string; envAuto: string };
  guard: { enabled: string; rules: string; builtinsOff: string; terminalHooks: string };
  classifier: { enabled: string; agent: string; deadline: string; onHarmful: string; provider: string; harness: string; model: string; effort: string };
  net: { denyHosts: string; allowHosts: string };
};
export declare const SCALARS: { redactor: string[]; guard: string[]; net: string[]; classifier: string[] };
export declare const REMEMBERED_REASON: string;
export declare function offWords(feature: "redactor" | "guard"): { tone: LineTone; text: string };
export declare function envDetectorWords(status: Pick<SecurityStatus, "env_auto" | "env_detectors"> | null | undefined): { tone: LineTone; text: string };
export declare function judgeWords(d: Pick<GuardDecision, "by" | "rule" | "reason">): string;
export declare const ACTIONS: readonly string[];
export declare const MATCHERS: readonly string[];
export declare const HOSTS: readonly string[];

export declare function blankRedactRule(): RedactRule;
export declare function blankGuardRule(): GuardRule;
export declare function slugOf(label: string): string;
export declare function redactRuleProblem(rule: RedactRule, taken?: readonly string[]): string | null;
export declare function guardRuleProblem(rule: GuardRule, taken?: readonly string[]): string | null;
export declare function moveRule<T>(list: readonly T[], i: number, dir: -1 | 1): T[];
/** A row's key for React, stable across edits of the rule. */
export declare function rowKey(rule: object): string;
/** `next`, under the row key `prev` had. */
export declare function keepRowKey<T extends object>(prev: object, next: T): T;
export declare function toggleBuiltin(off: readonly string[], id: string, enabled: boolean): string[];
export declare function verdictWords(verdict: string): { tone: LineTone; text: string };
export declare function actionWords(action: string): string;
export declare function matcherWords(matcher: GuardRule["matcher"]): string;
export declare function appliesWords(rule: Pick<GuardRule, "applies_to">): string;
export declare function ruleWords(rule: Pick<GuardRule, "action" | "matcher" | "applies_to">): string;
export declare function detectorWords(detector: RedactRule["detector"]): string;
export declare function harnessGuardWords(h: { tool_guard: boolean; input_rewrite: boolean }): { tone: LineTone; text: string };
export declare function readinessLine(status: SecurityStatus | null | undefined): { tone: LineTone; text: string };
export declare function problemLines(problems: readonly PolicyProblem[] | undefined, feature: "redactor" | "guard"): string[];
export declare function classifierFieldsFor(provider: string): string[];

/** The rules a rule editor shows for a scope: its open draft, or what the scope holds. */
export declare function draftOf<T>(drafts: Readonly<Partial<Record<string, T[]>>>, scope: string, stored: T[]): { rules: T[]; dirty: boolean };
export declare function withDraft<T>(drafts: Readonly<Partial<Record<string, T[]>>>, scope: string, rules: T[]): Partial<Record<string, T[]>>;
export declare function withoutDraft<T>(drafts: Readonly<Partial<Record<string, T[]>>>, scope: string): Partial<Record<string, T[]>>;

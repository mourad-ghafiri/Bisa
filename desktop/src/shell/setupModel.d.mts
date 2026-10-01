import type { ModelPlan, Readiness, ReadinessCheck, ReadinessFix } from "../types";

export type GatePlatform = "mac_os" | "linux" | "windows";
export type GateMode = "none" | "banner" | "modal";

/** What the gate knows: the last answer, and why the last read failed when it did. */
export interface GateState {
  readiness: Readiness | null;
  error: string | null;
}

/** What one read of the checks came to. */
export type ReadinessRead = { ok: true; readiness: Readiness } | { ok: false; error: string };

/** The one call a fix becomes. */
export type FixCall = { kind: "settings"; set: Record<string, unknown> } | { kind: "agent"; id: string; body: { harness: string; models: ModelPlan } };

export declare const RECHECK_MS: number;
export declare const UNREAD: GateState;
export declare const CHECK_IDS: readonly string[];
export declare function afterRead(was: GateState, read: ReadinessRead): GateState;
export declare function recheckMs(state: { readiness: { ready?: boolean } | null; error: string | null }): number | null;
export declare function movesReadiness(payload: { type?: string } | null | undefined): boolean;
export declare function platformOf(nav: { platform?: string; userAgent?: string } | null | undefined): GatePlatform;
export declare function commandsFor(hint: { commands?: readonly { platform: string; command: string }[] } | null | undefined, platform: string): string[];
export declare function gateMode(facts: { ready: boolean | null | undefined; screen: string | null | undefined }): GateMode;
export declare function progressWords(readiness: { checks?: readonly { state: string }[] } | null | undefined): string;
export declare function bannerWords(readiness: { checks?: readonly { state: string }[] } | null | undefined): string;
export declare function checkWords(check: { id: string; state: string } | null | undefined): { icon: "branch" | "harness" | "decisions" | "coreAgent" | "warn"; tone: "ok" | "danger" | "warn"; word: string };
export declare function doorTarget(door: { door: string; tab?: string } | null | undefined): { route: { name: "settings" | "agents" }; search?: { tab: string } } | null;
export declare function doorLabel(door: { door: string; tab?: string } | null | undefined): string;
export declare function fixBody(fix: { kind?: string; set?: Record<string, unknown>; agent?: string; harness?: string; models?: ModelPlan } | null | undefined): FixCall | null;
export declare function offeredFixes(check: Pick<ReadinessCheck, "fixes"> | null | undefined): ReadinessFix[];

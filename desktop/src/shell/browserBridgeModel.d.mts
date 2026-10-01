/** Types for `browserBridgeModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { AttachmentRef, BrowserConsoleLine, BrowserDialog, BrowserRequest, BrowserResult, BrowserScope, BrowserScrollPosition } from "../types";
import type { InspectorConsoleLine, InspectorDialog, InspectorScroll } from "../ui/artifact/pageInspector.mjs";
import type { IdeCentre } from "../views/_workbench/ideModeModel.mjs";
import type { BrowserSession, BrowsersState } from "./browsersModel.mjs";

/** How many taken-up requests the bridge remembers. */
export declare const HANDLED_KEPT: number;

export type BridgeRequest = BrowserRequest;
export type BridgeScope = BrowserScope;
export type BridgePlan =
  | { kind: "refuse"; error: string }
  | { kind: "open"; url: string; tab: string | null; home: { scope: string; id: string } | null; headless: boolean }
  | { kind: "tabs" }
  | { kind: "drive"; key: string; message: Record<string, unknown>; navigates: boolean }
  | { kind: "back"; key: string }
  | { kind: "forward"; key: string }
  | { kind: "reload"; key: string }
  | { kind: "wait-load"; key: string; timeoutMs: number }
  | { kind: "close"; key: string }
  | { kind: "screenshot"; key: string };
/** What the desktop answers the engine — the wire's `BrowserResult`, with the page's facts in the wire's shapes. */
export type BridgeResult = Omit<BrowserResult, "tabs" | "scroll" | "dialogs" | "console" | "value"> & {
  tabs?: { key: string; url: string; title: string }[];
  scroll?: BrowserScrollPosition;
  /** A script's value — any JSON the page gave, or a cut string. */
  value?: unknown;
  dialogs?: BrowserDialog[];
  console?: BrowserConsoleLine[];
};
export declare const DEFAULT_WAIT_MS: number;
export declare const MAX_WAIT_MS: number;
export declare const NAVIGATING: readonly string[];
export declare const PLACEHOLDER_ID: string;
export declare const PAGE_SILENT: string;
export declare const TAB_GONE: string;
export declare const PRESENCE_LAPSE_AFTER: number;
export declare function askAgain(plan: { navigates: boolean }, outcome: { kind: string }, attempt: number): boolean;
export declare function presenceLapsed(failures: number): boolean;
export declare const LOAD_LATE: string;
export declare const NOT_SHOWN: string;
export declare const SHOT_FAILED: string;
export declare function waitBound(ms: unknown): number;
export declare function rootFor(scope: BridgeScope | null | undefined, current: { scope: string; id: string } | null): { scope: string; id: string } | null;
export declare function revealPlan(session: BrowserSession | null | undefined, current: { scope: string; id: string } | null, centre: IdeCentre): "ide" | "pane" | null;
export declare function planRequest(request: BridgeRequest, scope: BridgeScope | null | undefined, state: BrowsersState, current: { scope: string; id: string } | null, headless?: boolean): BridgePlan;
export declare function tabsResult(sessions: readonly BrowserSession[]): BridgeResult;
export declare function tabResult(session: BrowserSession, extra?: Record<string, unknown>): BridgeResult;
export declare function navigatedResult(session: BrowserSession): BridgeResult;
export declare function screenshotResult(session: BrowserSession, screenshot: AttachmentRef, size: { width: number; height: number }): BridgeResult;
export declare function answerResult(
  session: BrowserSession,
  said: { ok?: boolean; error?: string | null; url?: string; title?: string; text?: string | null; selector?: string | null; count?: number | null; waitedMs?: number | null; scroll?: InspectorScroll | null; value?: unknown; dialogs?: readonly InspectorDialog[]; console?: readonly InspectorConsoleLine[] },
): BridgeResult;
/** The answer as the node takes it: the wire's `BrowserResult`, a script's value any JSON the page gave. */
export type WireBrowserResult = Omit<BrowserResult, "value" | "path"> & { value?: unknown };
export declare function wireResult(result: BridgeResult | Record<string, unknown> | null | undefined): WireBrowserResult;
export declare function browsersRootedAt(sessions: readonly BrowserSession[], scope: string, id: string): BrowserSession[];

/** Types for `browsersModel.mjs`, plain JavaScript so `node --test` reads it. */

import type { WorkbenchScope } from "../routeModel.mjs";

/** Where a tab may be at home: a workbench root's scope, a workflow, a channel, a direct message, or a conversation. */
export type BrowserScope = WorkbenchScope | "workflow" | "channel" | "dm" | "conversation";
export interface BrowserHome {
  readonly scope: BrowserScope;
  readonly id: string;
}
export declare const BROWSER_SESSIONS_KEY: string;
export declare const BROWSER_SCOPES: readonly BrowserScope[];
export declare const BLANK_URL: string;

/** Who opened a tab: a person's act, an agent's request (its id, when the request named one), a window a page asked for (the asking tab's key). */
export type BrowserOpener = { readonly kind: "person" } | { readonly kind: "agent"; readonly agent: string | null } | { readonly kind: "page"; readonly from: string };
export declare const OPENERS: readonly BrowserOpener["kind"][];
export declare const PERSON: BrowserOpener;
export declare function byAgent(agent: string | null | undefined): BrowserOpener;
export declare function byPage(from: string): BrowserOpener;
export declare function openerOf(raw: unknown): BrowserOpener | null;
/** Who opened a tab, in a row's words — `null` for a person's own. */
export declare function openerWords(by: BrowserOpener | null | undefined, agentName?: (id: string) => string | null | undefined): string | null;

export interface BrowserSession {
  readonly key: string;
  /** Where the tab is at home; `null` is the workspace's. */
  readonly home: BrowserHome | null;
  /** Who opened it — every tab is born from an act. */
  readonly by: BrowserOpener;
  readonly url: string;
  readonly title: string;
  readonly loading: boolean;
  /** The webview's own history: a page to go back to, one to go forward to. */
  readonly canBack: boolean;
  readonly canForward: boolean;
  /** Kept out of sight: no host draws it, and it is not made active (ide/18 §Headless tabs). */
  readonly headless: boolean;
  readonly pane: string | null;
}
/** What the webview says of a tab's history on a move. */
export interface BrowserHistory {
  readonly canBack: boolean;
  readonly canForward: boolean;
}
export interface BrowsersState {
  readonly sessions: readonly BrowserSession[];
  readonly active: string | null;
  readonly seq: number;
}
export interface BrowserTarget {
  /** Who opens the tab — required: a door that does not say opens nothing. */
  by: BrowserOpener;
  home?: { scope: string; id: string } | null;
  url?: string | null;
  pane?: string | null;
  headless?: boolean;
}
export declare const EMPTY_BROWSERS: BrowsersState;
export declare function normalizeUrl(input: string): { url: string } | { error: string };
export declare function urlWords(url: string): string;
export declare function workbenchHomeOf(session: { home?: { scope?: string; id?: string } | null } | null | undefined): { scope: "workstream" | "work_item" | "goal"; id: string } | null;
export declare function homeOf(home: { scope?: string; id?: string } | null | undefined): BrowserHome | null;
export declare function openBrowser(state: BrowsersState, target: BrowserTarget): BrowsersState;
export declare function sessionOf(state: BrowsersState, key: string): BrowserSession | null;
export declare function browsersRootedAt(sessions: readonly BrowserSession[], scope: string, id: string): BrowserSession[];
export declare function atHome(session: BrowserSession | null | undefined, place: { scope: string; id: string } | null): boolean;
export declare function draftScopeOf(session: BrowserSession | null | undefined): string;
export declare function navigated(state: BrowsersState, key: string, url: string, event: "started" | "finished", history?: BrowserHistory | null): BrowsersState;
export declare function titled(state: BrowsersState, key: string, title: string): BrowsersState;
export declare function asked(state: BrowsersState, key: string, url: string): BrowsersState;
export declare function closeAll(state: BrowsersState): BrowsersState;
export declare function closeBrowser(state: BrowsersState, key: string): BrowsersState;
export declare function focusBrowser(state: BrowsersState, key: string): BrowsersState;
export declare function reveal(state: BrowsersState, key: string): BrowsersState;
export declare function seenSessions(sessions: readonly BrowserSession[]): BrowserSession[];
export declare function seenRootedAt(sessions: readonly BrowserSession[], scope: string, id: string): BrowserSession[];
export declare function visibilityWords(session: BrowserSession | null | undefined): string | null;
export declare function browserLabel(session: BrowserSession | null | undefined): string;
export declare function browserTitle(session: BrowserSession | null | undefined): string;
export declare function serializeBrowsers(state: BrowsersState): unknown;
export declare function parseBrowsers(raw: unknown): BrowsersState;

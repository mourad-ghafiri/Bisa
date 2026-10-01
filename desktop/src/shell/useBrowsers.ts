/**
 * The browser tabs, one store (ide/18): which are open, where each is at
 * home, who opened it, what it shows, which is active — the rules
 * `browsersModel.mjs`'s; the tabs in sight remembered in `localStorage`
 * while `browser.remember_tabs` says so, so a restart brings back what a
 * person was looking at. The webviews are `BrowserPanel.tsx`'s, one
 * per session here, keyed by the session's key; a session removed here has
 * its webview closed there. Whether a tab may open at all is this machine's
 * word (`browser.enabled`, `browserPrefsStore.ts`) and the shell's.
 */

import { jsonPref, readPref, webStorage, writePref } from "./storedPrefModel.mjs";
import { useSyncExternalStore } from "react";
import { browserAvailable } from "../browser/session";
import { log } from "../log";
import { toaster } from "../ui";
import { browserPrefs } from "./browserPrefsStore";
import {
  BROWSER_SESSIONS_KEY,
  EMPTY_BROWSERS,
  asked,
  closeAll,
  closeBrowser,
  focusBrowser,
  navigated,
  openBrowser,
  parseBrowsers,
  reveal,
  serializeBrowsers,
  sessionOf,
  titled,
  urlWords,
} from "./browsersModel.mjs";
import type { BrowserHistory, BrowserSession, BrowserTarget, BrowsersState } from "./browsersModel.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * How long a load may stay on its way before the tab stops saying so: the
 * webview reports a load's start and its finish and never its failure, so a
 * page that never comes — a port nobody listens on — would read *loading*
 * for good. Past this the tab settles and the person is told once.
 */
const LOAD_GRACE_MS = 20_000;

function load(): BrowsersState {
  return readPref(webStorage(), BROWSER_SESSIONS_KEY, (raw) => parseBrowsers(jsonPref(raw)), EMPTY_BROWSERS);
}

let state: BrowsersState = load();
const listeners = new Set<() => void>();

/** Storage refused: the tabs live for the session. */
function remember(next: BrowsersState): void {
  writePref(webStorage(), BROWSER_SESSIONS_KEY, browserPrefs().remember ? serializeBrowsers(next) : null);
}

function set(next: BrowsersState): void {
  if (next === state) return;
  state = next;
  remember(next);
  for (const l of listeners) l();
}

/** The timer that settles a load nobody finished, per tab. */
const stalls = new Map<string, number>();

function settleLater(key: string, url: string): void {
  const before = stalls.get(key);
  if (before !== undefined) window.clearTimeout(before);
  stalls.set(
    key,
    window.setTimeout(() => {
      stalls.delete(key);
      const s = sessionOf(state, key);
      if (!s || !s.loading || s.url !== url) return;
      set(navigated(state, key, url, "finished"));
      toaster.error(t("shell-use-browsers-did-not-load", { url: urlWords(url) }));
    }, LOAD_GRACE_MS),
  );
}

function settled(key: string): void {
  const timer = stalls.get(key);
  if (timer === undefined) return;
  window.clearTimeout(timer);
  stalls.delete(key);
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useBrowsers(): BrowsersState {
  return useSyncExternalStore(subscribe, () => state, () => state);
}

/**
 * Every surface asks before it renders its button: outside the desktop
 * shell, turned off here, or before this machine's settings have been read
 * — the boot window shows no browser it cannot vouch for — there is no tab
 * to open.
 */
export function canOpenBrowser(): boolean {
  const prefs = browserPrefs();
  return browserAvailable() && prefs.read && prefs.enabled;
}

/** Why a tab cannot open, in a sentence — or `null` when it can. */
export function whyNotBrowser(): string | null {
  if (!browserAvailable()) return t("shell-use-browsers-embedded-browser-desktop-app-s-window");
  const prefs = browserPrefs();
  if (!prefs.read) return t("shell-use-browsers-settings-still-being-read");
  if (!prefs.enabled) return t("shell-use-browsers-embedded-browser-turned-off-settings-browser");
  return null;
}

export function browserSessions(): readonly BrowserSession[] {
  return state.sessions;
}

export function browserState(): BrowsersState {
  return state;
}

/**
 * Open a tab — always a new one — at home in a place (or the workspace's)
 * and answer its key; `null` when no tab may open here. **The one door a
 * tab is born through**, and every caller says who opens it (`by`): a
 * person's act, an agent's request, a page's window — never a pane that
 * merely mounted. A target naming no URL opens on the home page when the
 * workspace sets one. The birth is a line in the log, so a count that
 * surprises can be read back.
 */
export function openBrowserIn(target: BrowserTarget): string | null {
  if (!canOpenBrowser()) return null;
  const url = target.url ?? (browserPrefs().home || null);
  const next = openBrowser(state, { ...target, url });
  if (next === state) return null;
  set(next);
  const born = next.sessions[next.sessions.length - 1] ?? null;
  if (born) log.debug("browser", "a tab opened", { key: born.key, by: born.by.kind, home: born.home ? `${born.home.scope}:${born.home.id}` : "workspace", headless: born.headless });
  return born?.key ?? null;
}

export function closeBrowserTab(key: string): void {
  settled(key);
  set(closeBrowser(state, key));
}

export function focusBrowserTab(key: string): void {
  set(focusBrowser(state, key));
}

/** Show a tab kept out of sight — the person's act, never an agent's — and bring it to the front. */
export function revealBrowserTab(key: string): void {
  set(reveal(state, key));
}

/** The bar asked for a URL: the tab shows it loading until the webview says otherwise, or the grace runs out. */
export function browserAsked(key: string, url: string): void {
  set(asked(state, key, url));
  settleLater(key, url);
}

/** The webview said the page moved, and what its history is now. */
export function browserNavigated(key: string, url: string, event: "started" | "finished", history: BrowserHistory | null = null): void {
  set(navigated(state, key, url, event, history));
  if (event === "started") settleLater(key, url);
  else settled(key);
}

/** The page said its title. */
export function browserTitled(key: string, title: string): void {
  set(titled(state, key, title));
}

/** Close every tab at home in these workstreams — a retirement put them away. */
export function closeBrowsersRootedAt(workstreams: readonly string[]): void {
  let next = state;
  for (const s of state.sessions) {
    if (s.home?.scope === "workstream" && workstreams.includes(s.home.id)) next = closeBrowser(next, s.key);
  }
  set(next);
}

/**
 * The prefs moved: what is remembered follows them — forgotten at once when
 * remembering is off — every tab is closed when the browser is switched
 * off (the sequence kept, so no key is minted twice), and every reader of
 * the store hears it even when no tab moved, since whether a tab may open
 * at all is the store's word too.
 */
export function browserPrefsChanged(): void {
  remember(state);
  if (browserPrefs().read && !browserPrefs().enabled) set(closeAll(state));
  for (const l of listeners) l();
}

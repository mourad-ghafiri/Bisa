/**
 * A typed hash router, no dependency.
 *
 * Hash routing (not history) because the app is served from a Tauri webview
 * and a plain file origin — `#/…` needs no server rewrite rules and survives
 * a reload. Back/forward come free from the browser's own hash history.
 *
 * The hash carries a query string of its own (`#/channels/abc?aux=thread`).
 * That is what makes the right-hand pane and list selection *places*: Back
 * closes the pane instead of leaving the screen, and a reload lands you where
 * you were.
 *
 * **Every address is resolved before anything reads it.** The router stands
 * on a hash of its own (`address`), never on `window.location.hash`: what
 * the window says goes through the place memory first
 * (`shell/placeMemoryStore.land`), so a bare link to a goal opens it on the
 * tab it was left on, and no render — not even one between a link's click
 * and its `hashchange` — sees an address that has not been resolved.
 *
 * **An arrival or an exact entry, by who wrote it.** An entry the router
 * resolved carries a stamp in `history.state`. One that carries it — written
 * here, or reached by Back and Forward — is taken as it is; one that does
 * not — a link, an address typed — is an arrival, given what its path
 * remembers and then stamped in place, with no history entry of its own.
 * `navigate`, `replace` and `setSearch` write their entry themselves, so
 * nothing here waits on an event it caused.
 *
 * A launch has no hash: it opens on the last place the app was on, and on
 * the **home** when nothing is remembered — the first destination of your
 * sidebar order (`navOrderStore.homeRoute`), the Inbox until you move
 * something above it; a hash that names nothing lands there too.
 */

import { useCallback, useMemo, useSyncExternalStore } from "react";

import { applySearchPatch, href, indexRouteOf, parse, section, splitHash, WORKBENCH_SCOPES } from "./routeModel.mjs";
import type { Route, RouteName, SearchPatch, WorkbenchScope } from "./routeModel.mjs";
import { homeRoute } from "./shell/navOrderStore";
import { land, launchPlace } from "./shell/placeMemoryStore";

// The facts — which hash is which screen, and back — are `routeModel.mjs`'s,
// tested under `node --test`; this file holds the window and the hooks.
export { href, indexRouteOf, parse, section, WORKBENCH_SCOPES };
export type { Route, RouteName, SearchPatch, WorkbenchScope };

/** What an entry the router resolved carries in `history.state`. */
const STAMP = Object.freeze({ bisa: 1 });

/** Whether the entry the window stands on is one the router resolved. */
function stamped(): boolean {
  const state: unknown = window.history.state;
  return typeof state === "object" && state !== null && (state as { bisa?: unknown }).bisa === STAMP.bisa;
}

/** The hash the router stands on, resolved. */
let current = "";
let installed = false;
const subscribers = new Set<() => void>();

/** The window's URL with `hash` for its fragment. */
function urlOf(hash: string): string {
  return `${window.location.pathname}${window.location.search}${hash}`;
}

/** Stand on `hash`, and tell who reads the address when it moved. */
function stand(hash: string): void {
  if (hash === current) return;
  current = hash;
  for (const s of [...subscribers]) s();
}

/**
 * The window moved by a hand that is not the router's — a link, Back,
 * Forward, an address typed: resolve where it stands, stamp the entry in
 * place, and stand there.
 */
function resolveWindow(): void {
  const given = window.location.hash;
  const exact = stamped();
  const landed = land(given, exact);
  if (landed !== given || !exact) window.history.replaceState(STAMP, "", urlOf(landed));
  stand(landed);
}

/** Write an entry of the router's own, and stand on it. */
function write(hash: string, how: "push" | "replace"): void {
  if (how === "push") window.history.pushState(STAMP, "", urlOf(hash));
  else window.history.replaceState(STAMP, "", urlOf(hash));
  stand(hash);
  // The window's other listeners hear a move they would have heard from the
  // browser; the router's own finds the entry stamped and stands still.
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

/**
 * Begin routing: open where the app closed when the window has no hash,
 * resolve the hash it has otherwise, and hear every move from here on.
 * Called once, before the first render (`main.tsx`); a reader that asks
 * before then installs it.
 */
export function installRouter(): void {
  if (installed) return;
  installed = true;
  const launch = splitHash(window.location.hash).path === "/" ? launchPlace() : null;
  if (launch) {
    window.history.replaceState(STAMP, "", urlOf(launch));
    current = land(launch, true);
  } else {
    resolveWindow();
  }
  window.addEventListener("hashchange", resolveWindow);
}

/** The address the router stands on — resolved, never the window's raw hash. */
export function address(): string {
  installRouter();
  return current;
}

/**
 * Go to a place by its hash — a section's door, whose hash is where the
 * person was (`shell/sectionDoor.ts`). An arrival like any link's: a bare
 * path is given what it remembers.
 */
export function navigateTo(hash: string): void {
  installRouter();
  const next = land(hash, false);
  if (next !== current) write(next, "push");
}

export function navigate(route: Route, search?: SearchPatch): void {
  navigateTo(href(route, search));
}

/** Replace without a history entry (redirects, canonicalization). */
export function replace(route: Route, search?: SearchPatch): void {
  installRouter();
  write(land(href(route, search), false), "replace");
}

/**
 * Patch the current hash query, keeping the path. `undefined`/`null` removes
 * a key. Pushing by default, so opening a pane is one Back away from closed.
 * What it writes is exact: a filter cleared by hand stays cleared.
 */
export function setSearch(patch: SearchPatch, opts?: { replace?: boolean }): void {
  installRouter();
  const { path, query } = splitHash(current);
  const next = `#${path}${applySearchPatch(query, patch)}`;
  if (next === current) return;
  write(land(next, true), opts?.replace ? "replace" : "push");
}

export function back(): void {
  window.history.back();
}

export function forward(): void {
  window.history.forward();
}

function subscribe(cb: () => void): () => void {
  installRouter();
  subscribers.add(cb);
  return () => {
    subscribers.delete(cb);
  };
}

/** The resolved address, as a subscription — what a section's door measures itself against. */
export function useAddress(): string {
  return useSyncExternalStore(subscribe, address, () => "");
}

/**
 * The current route; re-renders on every hash change.
 *
 * **Memoised on the hash, and that is load-bearing rather than tidy.** `parse`
 * builds a fresh object, so returning it directly gave every caller a new
 * `Route` on every render — and a caller that put one in a dependency array
 * got an effect that re-ran forever. The notes panel did exactly that and
 * fetched once per response for as long as it was open.
 *
 * A `useMemo` per caller rather than one module-level cache: the parse is a
 * pure function of a global string, so a shared cache would be correct today
 * and quietly wrong the first time two roots render.
 */
export function useRoute(): Route {
  const hash = useAddress();
  return useMemo(() => parse(hash, homeRoute()), [hash]);
}

/** The route on screen right now, for a reader with no render to wait for — the same parse, the same home. */
export function currentRoute(): Route {
  return parse(address(), homeRoute());
}

/**
 * The current hash query; re-renders on every hash change.
 *
 * Memoised for the same reason `useRoute` is: a fresh `URLSearchParams` every
 * render is one dependency array away from the same loop.
 */
export function useSearchParams(): URLSearchParams {
  const hash = useAddress();
  return useMemo(() => new URLSearchParams(splitHash(hash).query), [hash]);
}

/** One search key, as a value + setter. */
export function useSearchValue(key: string): [string | null, (v: string | null) => void] {
  const params = useSearchParams();
  const set = useCallback((v: string | null) => setSearch({ [key]: v }), [key]);
  return [params.get(key), set];
}

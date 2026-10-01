/**
 * The embedded browser's tab as the shell holds it (ide/18): one native
 * child webview per tab, opened, moved, hidden, closed, driven and
 * snapshotted from here and nowhere else — `shell/BrowserPanel.tsx` owns
 * the webviews; the bar and the bridge drive them. The webview's own events
 * come back through `listenBrowserViews`.
 *
 * Nothing here names a file or a program: a tab is a URL and a key, and the
 * page's script is the kit's (`pageInspector.mjs`, `browserScript`, written with the keymap's browser chords as the tab opens). What
 * a page says comes back as `browser:message` through the shell's one door
 * on every page — no page has IPC.
 */

import { terminalAvailable } from "../terminal/session";

/** A tab's box in the main window, in CSS pixels. */
export interface BrowserBounds {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Whether this build can open a browser tab at all — the desktop shell. */
export function browserAvailable(): boolean {
  return terminalAvailable();
}

async function invoke<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  const core = await import("@tauri-apps/api/core");
  return core.invoke<T>(cmd, args);
}

/** The box a webview was drawn in, read back after a placement — `shown` false when it was put out of sight. */
export interface BrowserPlaced extends BrowserBounds {
  shown: boolean;
}

/** Open a tab's webview on a URL, the page's script aboard — hidden until its first placement. */
export function openBrowserView(key: string, url: string, script: string): Promise<void> {
  return invoke("browser_open", { key, url, script });
}
export function navigateBrowserView(key: string, url: string): Promise<void> {
  return invoke("browser_navigate", { key, url });
}
export function browserViewBack(key: string): Promise<void> {
  return invoke("browser_back", { key });
}
export function browserViewForward(key: string): Promise<void> {
  return invoke("browser_forward", { key });
}
export function browserViewReload(key: string): Promise<void> {
  return invoke("browser_reload", { key });
}
/** Stop a page on its way; the tab hears `finished` on the page it is on. */
export function browserViewStop(key: string): Promise<void> {
  return invoke("browser_stop", { key });
}
/** The main page's viewport in its own CSS pixels — `window.innerWidth` and `window.innerHeight` — sent with every placement so the shell anchors the tab on the page, not on the window (ide/18). */
export interface BrowserViewport {
  width: number;
  height: number;
}
/** Where the webview is and whether it shows; answers the box actually drawn, read back through the same anchor. */
export function setBrowserViewBounds(key: string, bounds: BrowserBounds, visible: boolean, viewport: BrowserViewport): Promise<BrowserPlaced> {
  return invoke("browser_bounds", { key, bounds, visible, viewport });
}
export function closeBrowserView(key: string): Promise<void> {
  return invoke("browser_close", { key });
}
/** Hand the page's script a request; the page answers through `browser:message`. */
export function driveBrowserView(key: string, request: Record<string, unknown>): Promise<void> {
  return invoke("browser_drive", { key, request });
}
/**
 * A PNG of the tab as it shows, `width` pixels wide — the webview's own
 * snapshot, never the screen. The tab must be showing: the caller shows it
 * first (`browserBridge.ts`, the bar's camera).
 */
export async function screenshotBrowserView(key: string, width: number): Promise<Uint8Array> {
  const bytes = await invoke<ArrayBuffer | Uint8Array | number[]>("browser_screenshot", { key, width });
  if (bytes instanceof Uint8Array) return bytes;
  if (bytes instanceof ArrayBuffer) return new Uint8Array(bytes);
  return Uint8Array.from(bytes);
}

export interface BrowserNavigated {
  key: string;
  url: string;
  event: "started" | "finished";
  /** The webview's own history, read as the page moved. */
  canBack: boolean;
  canForward: boolean;
}
export interface BrowserSaid {
  key: string;
  message: unknown;
}
/** The page said its title — the webview's own word, for every page. */
export interface BrowserTitled {
  key: string;
  title: string;
}
/** The page asked for a window — a link with a target, `window.open` — which is a tab of ours. */
export interface BrowserOpened {
  key: string;
  url: string;
}

export interface BrowserViewListeners {
  navigated: (e: BrowserNavigated) => void;
  said: (e: BrowserSaid) => void;
  titled: (e: BrowserTitled) => void;
  opened: (e: BrowserOpened) => void;
}

/** The webviews' events: a navigation, what a page said, its title, a window it asked for. Answers the unsubscribe. */
export async function listenBrowserViews(on: BrowserViewListeners): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const offs = await Promise.all([
    listen<BrowserNavigated>("browser:navigated", (e) => on.navigated(e.payload)),
    listen<BrowserSaid>("browser:message", (e) => on.said(e.payload)),
    listen<BrowserTitled>("browser:titled", (e) => on.titled(e.payload)),
    listen<BrowserOpened>("browser:new-window", (e) => on.opened(e.payload)),
  ]);
  return () => {
    for (const off of offs) off();
  };
}

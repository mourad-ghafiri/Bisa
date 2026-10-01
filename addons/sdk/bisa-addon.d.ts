/**
 * `window.bisa` — the one door an addon has into Bisa (18 — Addons).
 *
 * The platform serves `bisa-addon.js` at every addon's root; a bundle never
 * ships it. These declarations are for your editor: copy this file beside
 * your sources, or reference it with `/// <reference path="…/bisa-addon.d.ts" />`.
 *
 * Every method is a promise. A call the platform refuses rejects with an
 * `AddonError` whose `code` is one of the refusal codes below and whose
 * `message` is a sentence in the person's language. Nothing here reaches a
 * file, a program, the app's token or the network directly.
 */

export type PermissionWord =
  | "platform_info"
  | "theme"
  | "system_load"
  | "workspace_summary"
  | "notify"
  | "clipboard_write"
  | "storage"
  | "network"
  | "open_url"
  | "navigate";

export type RefusalCode =
  | "not_ready"
  | "unknown_method"
  | "permission"
  | "not_allowed"
  | "bad_params"
  | "too_large"
  | "rate_limited"
  | "quota"
  | "unavailable"
  | "refused";

export interface AddonError extends Error {
  name: "AddonError";
  code: RefusalCode;
}

/** What the platform says once, when it is ready. */
export interface Hello {
  addon: { id: string; name: string; version: string };
  /** The permissions the person granted, as words. */
  granted: PermissionWord[];
  /** The person's language tag, `en`. A language change reloads the window, so a page hears a new one in its next `ready`. */
  locale: string;
  theme: { scheme: "light" | "dark" };
  window: { width: number; height: number; resizable: boolean; closable: boolean; transparent: boolean; frame: "bar" | "none" };
}

/** The machine's load, at the footer's cadence (`system_load`). */
export interface SystemLoad {
  cpu_percent: number;
  load: [number, number, number];
  mem_used: number;
  mem_total: number;
  swap_used: number;
  swap_total: number;
  uptime_secs: number;
  gpu?: { util_percent: number; renderer_percent?: number | null; tiler_percent?: number | null; mem_used?: number | null } | null;
  /** The volume the platform's data lives on, and what the workspace weighs. */
  disk: { used: number; total: number; mount: string; workspace_bytes: number } | null;
}

/** What the workspace looks like (`workspace_summary`). */
export interface WorkspaceSummary {
  waiting: number;
  review: number;
  working: number;
}

/** A brokered fetch's answer (`network`). */
export interface FetchResult {
  status: number;
  contentType: string | null;
  /** The body as text, cut at 1 MiB. */
  body: string;
  truncated: boolean;
}

export type AddonRoute = "inbox" | "goals" | "pulse" | "workflows" | "projects" | "channels" | "messages" | "agents" | "teams" | "settings";

export interface Events {
  theme: { scheme: "light" | "dark" };
  "system.load": SystemLoad;
  "workspace.summary": WorkspaceSummary;
  visibility: { visible: boolean };
}

export interface Bisa {
  readonly version: 1;
  /** Resolves once the platform said it is ready; every call waits for this on its own. */
  ready(): Promise<Hello>;
  /** Whether the person granted a permission. */
  granted(word: PermissionWord): boolean;
  /** Listen to a topic; the return value unsubscribes. */
  on<K extends keyof Events>(topic: K, fn: (payload: Events[K]) => void): () => void;
  off<K extends keyof Events>(topic: K, fn: (payload: Events[K]) => void): void;
  /** Any method by name — the sugar below is the same call. */
  call(method: string, params?: Record<string, unknown>): Promise<unknown>;
  methods(): string[];
  events(): (keyof Events)[];

  platform: { info(): Promise<{ version: string; locale: string }> };
  theme: { get(): Promise<{ scheme: "light" | "dark" }> };
  system: { load: { subscribe(): Promise<{ subscribed: true }>; unsubscribe(): Promise<{ subscribed: false }> } };
  workspace: { summary(): Promise<WorkspaceSummary> };
  /** At most one notice every ten seconds; the title is cut at 100 characters, the body at 200. */
  notify: { show(opts: { title: string; body?: string }): Promise<{ shown: true }> };
  /** At most 64 KiB. */
  clipboard: { write(opts: { text: string } | string): Promise<{ written: true }> };
  /** A small store of strings, 256 KiB in all, kept by the platform on this machine. */
  storage: {
    get(key: string): Promise<{ value: string | null }>;
    set(key: string, value: string): Promise<{ stored: true }>;
    remove(key: string): Promise<{ removed: true }>;
    keys(): Promise<{ keys: string[] }>;
  };
  /** `https://` to a host the manifest names and the person allowed, through the platform; GET only. */
  network: { fetch(opts: { url: string; accept?: string } | string): Promise<FetchResult> };
  /** Asks the person; resolves `{opened: true}` only when they said yes. */
  url: { open(opts: { url: string } | string): Promise<{ opened: true }> };
  /** One of the app's screens. */
  navigate(opts: { route: AddonRoute } | AddonRoute): Promise<{ navigated: true }>;
  window: {
    /** Only when the manifest says `resizable`; clamped to its bounds. */
    resize(opts: { width: number; height: number }): Promise<{ asked: true }>;
    /** Only when the manifest says `closable`. */
    close(): Promise<{ closed: true }>;
    /** The bar's title, at most 60 characters. */
    setTitle(title: string): Promise<{ set: true }>;
  };
}

declare global {
  interface Window {
    bisa: Bisa;
  }
  const bisa: Bisa;
}

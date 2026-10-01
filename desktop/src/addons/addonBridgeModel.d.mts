/**
 * Types for `addonBridgeModel.mjs`, which is plain JavaScript so `node --test`
 * imports it without a build step.
 */

export const PROTOCOL_VERSION: 1;
export const MAX_ID_CHARS: number;
export const MAX_PARAMS_BYTES: number;
export const MAX_NOTIFY_TITLE_CHARS: number;
export const MAX_NOTIFY_BODY_CHARS: number;
export const NOTIFY_MIN_MS: number;
export const MAX_CLIPBOARD_BYTES: number;
export const STORAGE_QUOTA_BYTES: number;
export const MAX_STORAGE_KEY_CHARS: number;
export const MAX_URL_CHARS: number;
export const MAX_TITLE_CHARS: number;
export const MAX_FETCH_BODY_BYTES: number;
export const STORAGE_KEY_PREFIX: string;

export type PermissionWord = "platform_info" | "theme" | "system_load" | "workspace_summary" | "notify" | "clipboard_write" | "storage" | "network" | "open_url" | "navigate";
export const PERMISSIONS: readonly PermissionWord[];

export interface MethodSpec {
  permission: PermissionWord | null;
  needs?: "resizable" | "closable";
}
export const METHODS: Readonly<Record<string, MethodSpec>>;
export const METHOD_NAMES: readonly string[];
export type AddonEvent = "theme" | "system.load" | "workspace.summary" | "visibility";
export const EVENTS: readonly AddonEvent[];
export const ADDON_ROUTES: readonly string[];
export type RefusalCode = "not_ready" | "unknown_method" | "permission" | "not_allowed" | "bad_params" | "too_large" | "rate_limited" | "quota" | "unavailable" | "refused";
export const REFUSALS: readonly RefusalCode[];

export function permissionWord(value: unknown): PermissionWord | null;
export function networkHosts(permissions: readonly unknown[]): string[];
export function permissionWords(permissions: readonly unknown[]): PermissionWord[];
export function cut(s: unknown, max: number): string;

export type ParsedMessage =
  | { kind: "hello" }
  | { kind: "call"; id: string; method: string; params: Record<string, unknown> }
  | { kind: "bad"; id: string | null; code: RefusalCode };
export function parseAddonMessage(data: unknown): ParsedMessage | null;
export function paramsOf(method: string, params: Record<string, unknown>): { ok: true; value: Record<string, unknown> } | { ok: false; code: RefusalCode };

export interface SessionWindow {
  width: number;
  height: number;
  resizable: boolean;
  closable: boolean;
  transparent: boolean;
  frame: "bar" | "none";
}
export interface AddonSession {
  id: string;
  name: string;
  version: string;
  granted: PermissionWord[];
  hosts: string[];
  window: SessionWindow;
  ready: boolean;
  subscribed: boolean;
  lastNotifyAt: number;
  title: string | null;
}
export interface SessionSource {
  id: string;
  manifest: { name: string; version: string; permissions?: unknown[]; window?: object | null };
  granted?: unknown[];
}
export function newSession(addon: SessionSource): AddonSession;

export interface Refusal {
  code: RefusalCode | string;
  message: string;
}
export function refusal(code: string, message?: string): Refusal;
export function refusalWords(code: string): string;
export interface Reply {
  v: 1;
  kind: "bisa:reply";
  id: string;
  ok: boolean;
  result?: unknown;
  error?: Refusal;
}
export function okReply(id: string, result: unknown): Reply;
export function errorReply(id: string, code: string, message?: string): Reply;
export function helloMessage(session: AddonSession, ctx: { locale: string; scheme: string }): Record<string, unknown>;
export function eventMessage(topic: AddonEvent, payload: unknown): { v: 1; kind: "bisa:event"; topic: AddonEvent; payload: unknown };

export type StorageBag = Record<string, string>;
export function storageBag(raw: unknown): StorageBag;
export function storageBytes(bag: StorageBag): number;
export function storageSet(bag: StorageBag, key: string, value: string): { bag: StorageBag } | { error: "quota" };

export type Effect =
  | { type: "subscribe" }
  | { type: "unsubscribe" }
  | { type: "notify"; title: string; body: string }
  | { type: "clipboard"; text: string }
  | { type: "storage" }
  | { type: "fetch"; id: string; url: string; accept: string | null }
  | { type: "open_url"; id: string; url: string }
  | { type: "navigate"; route: string }
  | { type: "resize"; width: number; height: number }
  | { type: "close" }
  | { type: "title"; title: string };

export interface CallContext {
  now: number;
  storage: StorageBag;
  platform: { version: string; locale: string };
  scheme: string;
  summary: unknown;
}
export interface CallOutcome {
  session: AddonSession;
  reply: Reply | null;
  effect: Effect | null;
  storage: StorageBag;
}
export function handleCall(session: AddonSession, call: { id: string; method: string; params: Record<string, unknown> }, ctx: CallContext): CallOutcome;
export function fetchResult(r: { status: number; content_type?: string | null; body_text: string; truncated: boolean }): { status: number; contentType: string | null; body: string; truncated: boolean };
/** The code a reply refuses with, or null for one that answers — what the bridge logs. */
export declare function refusedCode(reply: unknown): string | null;

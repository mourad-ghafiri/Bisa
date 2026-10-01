/**
 * The bridge between an addon's frame and the app, as facts (18 — Addons).
 *
 * An addon's page runs in a sandboxed frame with an opaque origin and a
 * policy that lets it fetch nothing; the one door it has is `postMessage`
 * to this window, and this module is what stands behind that door: which
 * methods exist and what each needs the person to have granted, the shape
 * a message must have to be read at all, the caps on every field, the rate
 * limit, the storage quota, and the refusal a call gets when it fails one
 * of them. Nothing here touches a frame, a store or the network — the
 * impure bridge (`addonBridge.ts`) hands each message here, performs the
 * **effect** this answers with, and sends the reply back.
 *
 * Nothing an addon sends is ever rendered as HTML by the app: a string that
 * reaches a person is text, and cut to its cap here first.
 *
 * Plain `.mjs`, so `node --test` reads it; the library the addon runs
 * (`addons/sdk/bisa-addon.js`) mirrors `METHOD_NAMES` and `EVENTS`, and a
 * test holds the two lists equal.
 */

import { t } from "../i18n/l10n.mjs";

export const PROTOCOL_VERSION = 1;
export const MAX_ID_CHARS = 64;
export const MAX_PARAMS_BYTES = 16 * 1024;
export const MAX_NOTIFY_TITLE_CHARS = 100;
export const MAX_NOTIFY_BODY_CHARS = 200;
/** The least time between two notices of one addon. */
export const NOTIFY_MIN_MS = 10_000;
export const MAX_CLIPBOARD_BYTES = 64 * 1024;
/** The most one addon may keep, keys and values together. */
export const STORAGE_QUOTA_BYTES = 256 * 1024;
export const MAX_STORAGE_KEY_CHARS = 128;
export const MAX_URL_CHARS = 2048;
export const MAX_TITLE_CHARS = 60;
export const MAX_FETCH_BODY_BYTES = 1024 * 1024;
/** The localStorage key an addon's store lives under, followed by its id. */
export const STORAGE_KEY_PREFIX = "bisa.addons.storage.";

/** The permissions a manifest may declare and a person may grant — the words the node knows. */
export const PERMISSIONS = Object.freeze([
  "platform_info",
  "theme",
  "system_load",
  "workspace_summary",
  "notify",
  "clipboard_write",
  "storage",
  "network",
  "open_url",
  "navigate",
]);

/**
 * Every method, the permission it needs (`null` for none) and the window
 * flag it needs (`resizable`, `closable`). The registry is closed: a method
 * not here is refused before its params are read.
 */
export const METHODS = Object.freeze({
  "platform.info": { permission: "platform_info" },
  "theme.get": { permission: "theme" },
  "system.load.subscribe": { permission: "system_load" },
  "system.load.unsubscribe": { permission: "system_load" },
  "workspace.summary": { permission: "workspace_summary" },
  "notify.show": { permission: "notify" },
  "clipboard.write": { permission: "clipboard_write" },
  "storage.get": { permission: "storage" },
  "storage.set": { permission: "storage" },
  "storage.remove": { permission: "storage" },
  "storage.keys": { permission: "storage" },
  "network.fetch": { permission: "network" },
  "url.open": { permission: "open_url" },
  navigate: { permission: "navigate" },
  "window.resize": { permission: null, needs: "resizable" },
  "window.close": { permission: null, needs: "closable" },
  "window.setTitle": { permission: null },
});

export const METHOD_NAMES = Object.freeze(Object.keys(METHODS));

/** The topics the app pushes to an addon that listens. */
export const EVENTS = Object.freeze(["theme", "system.load", "workspace.summary", "visibility"]);

/** The screens an addon may take the person to — never a record's page. */
export const ADDON_ROUTES = Object.freeze(["inbox", "goals", "pulse", "workflows", "projects", "channels", "messages", "agents", "teams", "settings"]);

/** Every code a refusal carries. */
export const REFUSALS = Object.freeze(["not_ready", "unknown_method", "permission", "not_allowed", "bad_params", "too_large", "rate_limited", "quota", "unavailable", "refused"]);

/**
 * A permission as the manifest spells it — a word, or `{network: {hosts}}`
 * — as its word.
 * @param {unknown} value
 * @returns {string | null}
 */
export function permissionWord(value) {
  if (typeof value === "string") return PERMISSIONS.includes(value) ? value : null;
  if (value && typeof value === "object" && "network" in value) return "network";
  return null;
}

/**
 * The hosts a `network` permission names, when the list holds one.
 * @param {unknown[]} permissions
 * @returns {string[]}
 */
export function networkHosts(permissions) {
  for (const p of permissions) {
    if (p && typeof p === "object" && "network" in p) {
      const hosts = /** @type {{network: {hosts?: unknown}}} */ (p).network.hosts;
      return Array.isArray(hosts) ? hosts.filter((h) => typeof h === "string") : [];
    }
  }
  return [];
}

/** @param {unknown[]} permissions */
export function permissionWords(permissions) {
  return permissions.map(permissionWord).filter((w) => w !== null);
}

/** @param {string} s @param {number} max */
export function cut(s, max) {
  const text = String(s ?? "");
  return text.length > max ? text.slice(0, max) : text;
}

function byteLength(s) {
  return new TextEncoder().encode(s).length;
}

/**
 * What arrived on the door, read only as far as its shape allows.
 * @param {unknown} data
 * @returns {{kind: "hello"} | {kind: "call", id: string, method: string, params: Record<string, unknown>} | {kind: "bad", id: string | null, code: string} | null}
 */
export function parseAddonMessage(data) {
  if (!data || typeof data !== "object") return null;
  const m = /** @type {Record<string, unknown>} */ (data);
  if (m.v !== PROTOCOL_VERSION || typeof m.kind !== "string") return null;
  if (m.kind === "bisa:hello") return { kind: "hello" };
  if (m.kind !== "bisa:call") return null;
  const id = typeof m.id === "string" && m.id.length > 0 && m.id.length <= MAX_ID_CHARS ? m.id : null;
  if (id === null) return { kind: "bad", id: null, code: "bad_params" };
  if (typeof m.method !== "string" || !METHOD_NAMES.includes(m.method)) return { kind: "bad", id, code: "unknown_method" };
  const params = m.params && typeof m.params === "object" && !Array.isArray(m.params) ? /** @type {Record<string, unknown>} */ (m.params) : {};
  let size = 0;
  try {
    size = byteLength(JSON.stringify(params));
  } catch {
    return { kind: "bad", id, code: "bad_params" };
  }
  if (size > MAX_PARAMS_BYTES) return { kind: "bad", id, code: "too_large" };
  return { kind: "call", id, method: m.method, params };
}

function isHttpUrl(s) {
  if (typeof s !== "string" || s.length === 0 || s.length > MAX_URL_CHARS) return false;
  try {
    const u = new URL(s);
    return u.protocol === "https:" || u.protocol === "http:";
  } catch {
    return false;
  }
}

function finiteInt(n) {
  return typeof n === "number" && Number.isFinite(n) && Math.floor(n) === n;
}

/**
 * A method's params, bounded and typed, or the refusal.
 * @param {string} method
 * @param {Record<string, unknown>} params
 * @returns {{ok: true, value: Record<string, unknown>} | {ok: false, code: string}}
 */
export function paramsOf(method, params) {
  const bad = { ok: false, code: "bad_params" };
  switch (method) {
    case "notify.show": {
      if (typeof params.title !== "string" || params.title.trim() === "") return bad;
      const body = typeof params.body === "string" ? params.body : "";
      return { ok: true, value: { title: cut(params.title, MAX_NOTIFY_TITLE_CHARS), body: cut(body, MAX_NOTIFY_BODY_CHARS) } };
    }
    case "clipboard.write": {
      if (typeof params.text !== "string") return bad;
      if (byteLength(params.text) > MAX_CLIPBOARD_BYTES) return { ok: false, code: "too_large" };
      return { ok: true, value: { text: params.text } };
    }
    case "storage.get":
    case "storage.remove": {
      if (typeof params.key !== "string" || params.key === "" || params.key.length > MAX_STORAGE_KEY_CHARS) return bad;
      return { ok: true, value: { key: params.key } };
    }
    case "storage.set": {
      if (typeof params.key !== "string" || params.key === "" || params.key.length > MAX_STORAGE_KEY_CHARS) return bad;
      if (typeof params.value !== "string") return bad;
      return { ok: true, value: { key: params.key, value: params.value } };
    }
    case "network.fetch": {
      if (!isHttpUrl(params.url)) return bad;
      const accept = typeof params.accept === "string" ? cut(params.accept, 200) : null;
      return { ok: true, value: { url: params.url, accept } };
    }
    case "url.open": {
      if (!isHttpUrl(params.url)) return bad;
      return { ok: true, value: { url: params.url } };
    }
    case "navigate": {
      if (typeof params.route !== "string" || !ADDON_ROUTES.includes(params.route)) return bad;
      return { ok: true, value: { route: params.route } };
    }
    case "window.resize": {
      if (!finiteInt(params.width) || !finiteInt(params.height)) return bad;
      return { ok: true, value: { width: params.width, height: params.height } };
    }
    case "window.setTitle": {
      if (typeof params.title !== "string") return bad;
      return { ok: true, value: { title: cut(params.title, MAX_TITLE_CHARS) } };
    }
    default:
      return { ok: true, value: {} };
  }
}

/**
 * One addon's conversation with the app, as the app remembers it.
 * @param {{id: string, manifest: {name: string, version: string, permissions?: unknown[], window: Record<string, unknown>}, granted?: unknown[]}} addon
 * @returns {AddonSession}
 */
export function newSession(addon) {
  const w = addon.manifest.window ?? {};
  return {
    id: addon.id,
    name: cut(addon.manifest.name, MAX_TITLE_CHARS),
    version: String(addon.manifest.version ?? ""),
    granted: permissionWords(addon.granted ?? []),
    hosts: networkHosts(addon.granted ?? []),
    window: {
      width: Number(w.width) || 0,
      height: Number(w.height) || 0,
      resizable: w.resizable !== false,
      closable: w.closable !== false,
      transparent: w.transparent === true,
      frame: w.frame === "none" ? "none" : "bar",
    },
    ready: false,
    subscribed: false,
    lastNotifyAt: Number.NEGATIVE_INFINITY,
    title: null,
  };
}

/** @typedef {ReturnType<typeof newSession>} AddonSession */

/** @param {string} code @param {string} [message] */
export function refusal(code, message) {
  return { code, message: message ?? refusalWords(code) };
}

/** The sentence for a refusal code. @param {string} code */
export function refusalWords(code) {
  switch (code) {
    case "not_ready":
      return t("addons-addon-bridge-refused-not-ready");
    case "unknown_method":
      return t("addons-addon-bridge-refused-unknown-method");
    case "permission":
      return t("addons-addon-bridge-refused-permission");
    case "not_allowed":
      return t("addons-addon-bridge-refused-not-allowed");
    case "bad_params":
      return t("addons-addon-bridge-refused-bad-params");
    case "too_large":
      return t("addons-addon-bridge-refused-too-large");
    case "rate_limited":
      return t("addons-addon-bridge-refused-rate-limited");
    case "quota":
      return t("addons-addon-bridge-refused-quota");
    case "unavailable":
      return t("addons-addon-bridge-refused-unavailable");
    default:
      return t("addons-addon-bridge-refused");
  }
}

/** @param {string} id @param {unknown} result */
export function okReply(id, result) {
  return { v: PROTOCOL_VERSION, kind: "bisa:reply", id, ok: true, result: result ?? null };
}

/** @param {string} id @param {string} code @param {string} [message] */
export function errorReply(id, code, message) {
  return { v: PROTOCOL_VERSION, kind: "bisa:reply", id, ok: false, error: refusal(code, message) };
}

/**
 * The code a reply refuses with, or `null` for one that answers — what the
 * bridge writes to the diagnostic log beside the method, so a developer
 * testing an addon reads every refusal with its code (guide/addons.md).
 * @param {unknown} reply
 * @returns {string | null}
 */
export function refusedCode(reply) {
  const r = /** @type {{kind?: unknown, ok?: unknown, error?: {code?: unknown}} | null} */ (reply && typeof reply === "object" ? reply : null);
  if (!r || r.kind !== "bisa:reply" || r.ok !== false) return null;
  return typeof r.error?.code === "string" ? r.error.code : "refused";
}

/** What the app says first, once the addon said hello. @param {AddonSession} session @param {{locale: string, scheme: string}} ctx */
export function helloMessage(session, ctx) {
  return {
    v: PROTOCOL_VERSION,
    kind: "bisa:ready",
    addon: { id: session.id, name: session.name, version: session.version },
    granted: session.granted.slice(),
    locale: ctx.locale,
    theme: { scheme: ctx.scheme },
    window: { ...session.window },
  };
}

/** A topic the addon may listen to. @param {string} topic @param {unknown} payload */
export function eventMessage(topic, payload) {
  if (!EVENTS.includes(topic)) throw new Error(`not an addon event: ${topic}`);
  return { v: PROTOCOL_VERSION, kind: "bisa:event", topic, payload: payload ?? null };
}

// ---------------------------------------------------------------------------
// Storage: one JSON bag per addon, under a quota
// ---------------------------------------------------------------------------

/** The bag a stored string holds — an object of strings, or empty. @param {unknown} raw */
export function storageBag(raw) {
  if (typeof raw !== "string" || raw === "") return {};
  try {
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    /** @type {Record<string, string>} */
    const out = {};
    for (const [k, v] of Object.entries(parsed)) if (typeof v === "string") out[k] = v;
    return out;
  } catch {
    return {};
  }
}

/** Keys and values together, in bytes. @param {Record<string, string>} bag */
export function storageBytes(bag) {
  let n = 0;
  for (const [k, v] of Object.entries(bag)) n += byteLength(k) + byteLength(v);
  return n;
}

/**
 * The bag with one more entry, or the quota's refusal — the bag handed in is
 * never changed.
 * @param {Record<string, string>} bag @param {string} key @param {string} value
 * @returns {{bag: Record<string, string>} | {error: "quota"}}
 */
export function storageSet(bag, key, value) {
  const next = { ...bag, [key]: value };
  if (storageBytes(next) > STORAGE_QUOTA_BYTES) return { error: "quota" };
  return { bag: next };
}

// ---------------------------------------------------------------------------
// One call
// ---------------------------------------------------------------------------

/**
 * Judge and answer one call. The answer is a reply to send now, or an
 * **effect** the bridge performs — one that answers later (`fetch`,
 * `open_url`) carries the call's id and no reply here.
 *
 * @param {AddonSession} session
 * @param {{id: string, method: string, params: Record<string, unknown>}} call
 * @param {{now: number, storage: Record<string, string>, platform: {version: string, locale: string}, scheme: string, summary: unknown}} ctx
 * @returns {{session: AddonSession, reply: object | null, effect: Record<string, unknown> | null, storage: Record<string, string>}}
 */
export function handleCall(session, call, ctx) {
  const same = { session, reply: null, effect: null, storage: ctx.storage };
  if (!session.ready) return { ...same, reply: errorReply(call.id, "not_ready") };
  const spec = METHODS[call.method];
  if (!spec) return { ...same, reply: errorReply(call.id, "unknown_method") };
  if (spec.permission && !session.granted.includes(spec.permission)) return { ...same, reply: errorReply(call.id, "permission") };
  if (spec.needs && !session.window[spec.needs]) return { ...same, reply: errorReply(call.id, "not_allowed") };
  const parsed = paramsOf(call.method, call.params);
  if (!parsed.ok) return { ...same, reply: errorReply(call.id, parsed.code) };
  const p = parsed.value;
  switch (call.method) {
    case "platform.info":
      return { ...same, reply: okReply(call.id, { version: ctx.platform.version, locale: ctx.platform.locale }) };
    case "theme.get":
      return { ...same, reply: okReply(call.id, { scheme: ctx.scheme }) };
    case "system.load.subscribe":
      return { ...same, session: { ...session, subscribed: true }, reply: okReply(call.id, { subscribed: true }), effect: { type: "subscribe" } };
    case "system.load.unsubscribe":
      return { ...same, session: { ...session, subscribed: false }, reply: okReply(call.id, { subscribed: false }), effect: { type: "unsubscribe" } };
    case "workspace.summary":
      return { ...same, reply: okReply(call.id, ctx.summary ?? null) };
    case "notify.show": {
      if (ctx.now - session.lastNotifyAt < NOTIFY_MIN_MS) return { ...same, reply: errorReply(call.id, "rate_limited") };
      return { ...same, session: { ...session, lastNotifyAt: ctx.now }, reply: okReply(call.id, { shown: true }), effect: { type: "notify", title: p.title, body: p.body } };
    }
    case "clipboard.write":
      return { ...same, reply: okReply(call.id, { written: true }), effect: { type: "clipboard", text: p.text } };
    case "storage.get":
      return { ...same, reply: okReply(call.id, { value: Object.hasOwn(ctx.storage, /** @type {string} */ (p.key)) ? ctx.storage[/** @type {string} */ (p.key)] : null }) };
    case "storage.set": {
      const set = storageSet(ctx.storage, /** @type {string} */ (p.key), /** @type {string} */ (p.value));
      if ("error" in set) return { ...same, reply: errorReply(call.id, "quota") };
      return { ...same, storage: set.bag, reply: okReply(call.id, { stored: true }), effect: { type: "storage" } };
    }
    case "storage.remove": {
      const next = { ...ctx.storage };
      delete next[/** @type {string} */ (p.key)];
      return { ...same, storage: next, reply: okReply(call.id, { removed: true }), effect: { type: "storage" } };
    }
    case "storage.keys":
      return { ...same, reply: okReply(call.id, { keys: Object.keys(ctx.storage).sort() }) };
    case "network.fetch":
      // Never answered here: the node judges the URL and fetches it.
      return { ...same, effect: { type: "fetch", id: call.id, url: p.url, accept: p.accept } };
    case "url.open":
      // Never answered here: the person is asked first.
      return { ...same, effect: { type: "open_url", id: call.id, url: p.url } };
    case "navigate":
      return { ...same, reply: okReply(call.id, { navigated: true }), effect: { type: "navigate", route: p.route } };
    case "window.resize":
      return { ...same, reply: okReply(call.id, { asked: true }), effect: { type: "resize", width: p.width, height: p.height } };
    case "window.close":
      return { ...same, reply: okReply(call.id, { closed: true }), effect: { type: "close" } };
    case "window.setTitle":
      return { ...same, session: { ...session, title: /** @type {string} */ (p.title) }, reply: okReply(call.id, { set: true }), effect: { type: "title", title: p.title } };
    default:
      return { ...same, reply: errorReply(call.id, "unknown_method") };
  }
}

/** The body the broker answered, cut to the cap the frame is handed. @param {{status: number, content_type?: string | null, body_text: string, truncated: boolean}} r */
export function fetchResult(r) {
  const body = cut(r.body_text, MAX_FETCH_BODY_BYTES);
  return { status: r.status, contentType: r.content_type ?? null, body, truncated: r.truncated || body.length < String(r.body_text ?? "").length };
}

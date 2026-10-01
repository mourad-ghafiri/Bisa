import test from "node:test";
import assert from "node:assert/strict";
import {
  ADDON_ROUTES,
  EVENTS,
  MAX_CLIPBOARD_BYTES,
  MAX_FETCH_BODY_BYTES,
  MAX_NOTIFY_BODY_CHARS,
  MAX_NOTIFY_TITLE_CHARS,
  MAX_PARAMS_BYTES,
  MAX_STORAGE_KEY_CHARS,
  METHODS,
  METHOD_NAMES,
  NOTIFY_MIN_MS,
  PERMISSIONS,
  PROTOCOL_VERSION,
  REFUSALS,
  STORAGE_QUOTA_BYTES,
  paramsOf,
  refusal,
  errorReply,
  eventMessage,
  fetchResult,
  handleCall,
  helloMessage,
  networkHosts,
  newSession,
  parseAddonMessage,
  permissionWords,
  refusalWords,
  storageBag,
  storageBytes,
  storageSet,
} from "./addonBridgeModel.mjs";
import { ROUTE_NAMES } from "../routeModel.mjs";

const addon = (granted = [], window = {}) => ({
  id: "acme.byte",
  manifest: { name: "Byte", version: "1.0.0", permissions: granted, window: { width: 200, height: 120, ...window } },
  granted,
});

const ctx = (over = {}) => ({ now: 100_000, storage: {}, platform: { version: "0.1.0", locale: "en" }, scheme: "dark", summary: { waiting: 1, review: 2, working: 3 }, ...over });

const ready = (a = addon()) => ({ ...newSession(a), ready: true });
const call = (method, params = {}, id = "c1") => ({ id, method, params });

test("the registry names every method's permission, and the routes are real screens", () => {
  assert.equal(METHOD_NAMES.length, 17);
  for (const name of METHOD_NAMES) assert.ok(name in METHODS);
  for (const spec of Object.values(METHODS)) assert.ok(spec.permission === null || PERMISSIONS.includes(spec.permission), JSON.stringify(spec));
  assert.equal(METHODS["window.resize"].needs, "resizable");
  assert.equal(METHODS["window.close"].needs, "closable");
  for (const route of ADDON_ROUTES) assert.ok(ROUTE_NAMES.includes(route), route);
  assert.ok(!ADDON_ROUTES.includes("goal") && !ADDON_ROUTES.includes("workbench"), "never a record's page");
  assert.deepEqual([...ADDON_ROUTES], ["inbox", "goals", "pulse", "workflows", "projects", "channels", "messages", "agents", "teams", "settings"], "ten screens; the Triggers screen is gone");
  assert.deepEqual([...EVENTS], ["theme", "system.load", "workspace.summary", "visibility"]);
  assert.ok(Object.isFrozen(EVENTS) && Object.isFrozen(METHODS));
});

test("the caps and the codes are the reference's, and params are judged per method", () => {
  assert.equal(PROTOCOL_VERSION, 1);
  assert.deepEqual([MAX_NOTIFY_TITLE_CHARS, MAX_NOTIFY_BODY_CHARS, MAX_CLIPBOARD_BYTES, MAX_STORAGE_KEY_CHARS, MAX_FETCH_BODY_BYTES], [100, 200, 64 * 1024, 128, 1024 * 1024]);
  assert.deepEqual([...REFUSALS], ["not_ready", "unknown_method", "permission", "not_allowed", "bad_params", "too_large", "rate_limited", "quota", "unavailable", "refused"]);
  for (const code of REFUSALS) assert.equal(refusal(code).code, code);
  assert.deepEqual(paramsOf("clipboard.write", { text: "x".repeat(MAX_CLIPBOARD_BYTES + 1) }), { ok: false, code: "too_large" });
  assert.deepEqual(paramsOf("storage.get", { key: "k".repeat(MAX_STORAGE_KEY_CHARS + 1) }), { ok: false, code: "bad_params" });
  assert.deepEqual(paramsOf("theme.get", { anything: 1 }), { ok: true, value: {} });
  assert.deepEqual(paramsOf("network.fetch", { url: "https://a.example/x", accept: 7 }), { ok: true, value: { url: "https://a.example/x", accept: null } });
});

test("a message is read only as far as its shape allows", () => {
  assert.equal(parseAddonMessage(null), null);
  assert.equal(parseAddonMessage("bisa:hello"), null);
  assert.equal(parseAddonMessage({ v: 2, kind: "bisa:hello" }), null, "another version is not ours");
  assert.deepEqual(parseAddonMessage({ v: 1, kind: "bisa:hello" }), { kind: "hello" });
  assert.equal(parseAddonMessage({ v: 1, kind: "bisa:reply", id: "x" }), null, "a reply is ours to send, not to hear");
  assert.deepEqual(parseAddonMessage({ v: 1, kind: "bisa:call", id: "", method: "theme.get" }), { kind: "bad", id: null, code: "bad_params" });
  assert.deepEqual(parseAddonMessage({ v: 1, kind: "bisa:call", id: "x".repeat(65), method: "theme.get" }), { kind: "bad", id: null, code: "bad_params" });
  assert.deepEqual(parseAddonMessage({ v: 1, kind: "bisa:call", id: "c1", method: "fs.read" }), { kind: "bad", id: "c1", code: "unknown_method" });
  const big = { text: "x".repeat(MAX_PARAMS_BYTES + 1) };
  assert.deepEqual(parseAddonMessage({ v: 1, kind: "bisa:call", id: "c1", method: "clipboard.write", params: big }), { kind: "bad", id: "c1", code: "too_large" });
  assert.deepEqual(parseAddonMessage({ v: 1, kind: "bisa:call", id: "c1", method: "theme.get", params: [1] }), { kind: "call", id: "c1", method: "theme.get", params: {} });
});

test("nothing is answered before hello, and a method needs its grant", () => {
  const cold = newSession(addon(["theme"]));
  assert.equal(handleCall(cold, call("theme.get"), ctx()).reply.error.code, "not_ready");
  const s = ready(addon(["theme"]));
  assert.deepEqual(handleCall(s, call("theme.get"), ctx()).reply, { v: 1, kind: "bisa:reply", id: "c1", ok: true, result: { scheme: "dark" } });
  assert.equal(handleCall(s, call("storage.get", { key: "k" }), ctx()).reply.error.code, "permission");
  assert.equal(handleCall(s, call("notify.show", { title: "t" }), ctx()).reply.error.code, "permission");
  assert.equal(handleCall(s, call("fs.read"), ctx()).reply.error.code, "unknown_method");
  assert.equal(refusalWords("permission"), refusalWords("permission"), "a sentence, the same every time");
  assert.notEqual(refusalWords("permission"), "");
});

test("a window flag the manifest denies refuses the move, whatever was granted", () => {
  const fixed = ready(addon([], { resizable: false, closable: false }));
  assert.equal(handleCall(fixed, call("window.resize", { width: 300, height: 200 }), ctx()).reply.error.code, "not_allowed");
  assert.equal(handleCall(fixed, call("window.close"), ctx()).reply.error.code, "not_allowed");
  const free = ready(addon());
  const out = handleCall(free, call("window.resize", { width: 300, height: 200 }), ctx());
  assert.deepEqual(out.effect, { type: "resize", width: 300, height: 200 });
  assert.equal(handleCall(free, call("window.resize", { width: "big", height: 1 }), ctx()).reply.error.code, "bad_params");
  assert.deepEqual(handleCall(free, call("window.close"), ctx()).effect, { type: "close" });
  const titled = handleCall(free, call("window.setTitle", { title: "x".repeat(80) }), ctx());
  assert.equal(titled.session.title.length, 60, "a title is cut to its cap");
});

test("a notice is rate-limited per addon and its words are cut", () => {
  const s = ready(addon(["notify"]));
  const first = handleCall(s, call("notify.show", { title: "T".repeat(150), body: "B".repeat(300) }), ctx({ now: 1000 }));
  assert.equal(first.effect.type, "notify");
  assert.equal(first.effect.title.length, 100);
  assert.equal(first.effect.body.length, 200);
  const second = handleCall(first.session, call("notify.show", { title: "again" }, "c2"), ctx({ now: 1000 + NOTIFY_MIN_MS - 1 }));
  assert.equal(second.reply.error.code, "rate_limited");
  assert.equal(second.effect, null);
  const later = handleCall(first.session, call("notify.show", { title: "again" }, "c3"), ctx({ now: 1000 + NOTIFY_MIN_MS }));
  assert.equal(later.effect.type, "notify");
  assert.equal(handleCall(s, call("notify.show", { title: "   " }), ctx()).reply.error.code, "bad_params");
});

test("storage is one bag under a quota, and never the app's own storage", () => {
  const s = ready(addon(["storage"]));
  let storage = {};
  const set = handleCall(s, call("storage.set", { key: "city", value: "Lisbon" }), ctx({ storage }));
  assert.equal(set.reply.ok, true);
  assert.deepEqual(set.effect, { type: "storage" });
  storage = set.storage;
  assert.deepEqual(handleCall(s, call("storage.get", { key: "city" }), ctx({ storage })).reply.result, { value: "Lisbon" });
  assert.deepEqual(handleCall(s, call("storage.get", { key: "nope" }), ctx({ storage })).reply.result, { value: null });
  assert.deepEqual(handleCall(s, call("storage.keys"), ctx({ storage })).reply.result, { keys: ["city"] });
  assert.equal(handleCall(s, call("storage.set", { key: "k", value: 42 }), ctx({ storage })).reply.error.code, "bad_params", "values are strings");
  assert.equal(handleCall(s, call("storage.set", { key: "", value: "x" }), ctx({ storage })).reply.error.code, "bad_params");
  const removed = handleCall(s, call("storage.remove", { key: "city" }), ctx({ storage }));
  assert.deepEqual(removed.storage, {});
  assert.deepEqual(storage, { city: "Lisbon" }, "the bag handed in is never changed");
  const big = storageSet({}, "k", "x".repeat(STORAGE_QUOTA_BYTES));
  assert.deepEqual(big, { error: "quota" });
  const fits = storageSet({}, "k", "x".repeat(STORAGE_QUOTA_BYTES - 1));
  assert.equal(storageBytes(fits.bag), STORAGE_QUOTA_BYTES);
  assert.deepEqual(storageBag('{"a":"1","b":2,"c":null}'), { a: "1" }, "only strings survive a read");
  assert.deepEqual(storageBag("not json"), {});
});

test("the network goes only through the broker, and a link only through the person", () => {
  const s = ready(addon([{ network: { hosts: ["api.example.com"] } }, "open_url", "navigate"]));
  assert.deepEqual(s.hosts, ["api.example.com"]);
  const fetch = handleCall(s, call("network.fetch", { url: "https://api.example.com/x" }), ctx());
  assert.equal(fetch.reply, null, "answered by the node, never here");
  assert.deepEqual(fetch.effect, { type: "fetch", id: "c1", url: "https://api.example.com/x", accept: null });
  assert.equal(handleCall(s, call("network.fetch", { url: "ftp://x" }), ctx()).reply.error.code, "bad_params");
  assert.equal(handleCall(s, call("network.fetch", { url: "javascript:alert(1)" }), ctx()).reply.error.code, "bad_params");
  const open = handleCall(s, call("url.open", { url: "https://example.com/docs" }), ctx());
  assert.equal(open.reply, null);
  assert.deepEqual(open.effect, { type: "open_url", id: "c1", url: "https://example.com/docs" });
  assert.deepEqual(handleCall(s, call("navigate", { route: "inbox" }), ctx()).effect, { type: "navigate", route: "inbox" });
  assert.equal(handleCall(s, call("navigate", { route: "goal" }), ctx()).reply.error.code, "bad_params");
  assert.equal(handleCall(s, call("navigate", { route: "workbench" }), ctx()).reply.error.code, "bad_params");
  assert.equal(handleCall(s, call("navigate", { route: "triggers" }), ctx()).reply.error.code, "bad_params", "a screen that is gone is no place to go");
});

test("the hello carries the grants as words and the window as the manifest shaped it", () => {
  const s = ready(addon(["theme", { network: { hosts: ["a.example"] } }], { frame: "none", transparent: true, resizable: false }));
  const hello = helloMessage(s, { locale: "en", scheme: "light" });
  assert.deepEqual(hello.granted, ["theme", "network"]);
  assert.deepEqual(hello.window, { width: 200, height: 120, resizable: false, closable: true, transparent: true, frame: "none" });
  assert.equal(hello.kind, "bisa:ready");
  assert.deepEqual(permissionWords(["storage", { network: { hosts: [] } }, "bogus", 7]), ["storage", "network"]);
  assert.deepEqual(networkHosts(["storage"]), []);
  assert.deepEqual(eventMessage("visibility", { visible: false }), { v: 1, kind: "bisa:event", topic: "visibility", payload: { visible: false } });
  assert.throws(() => eventMessage("secrets", {}));
  assert.equal(errorReply("c9", "quota").error.code, "quota");
});

test("the summary and the load are read-only answers; the system subscription is a flag", () => {
  const s = ready(addon(["workspace_summary", "system_load", "platform_info"]));
  assert.deepEqual(handleCall(s, call("workspace.summary"), ctx()).reply.result, { waiting: 1, review: 2, working: 3 });
  assert.deepEqual(handleCall(s, call("platform.info"), ctx()).reply.result, { version: "0.1.0", locale: "en" });
  const on = handleCall(s, call("system.load.subscribe"), ctx());
  assert.equal(on.session.subscribed, true);
  assert.deepEqual(on.effect, { type: "subscribe" });
  const off = handleCall(on.session, call("system.load.unsubscribe"), ctx());
  assert.equal(off.session.subscribed, false);
});

test("a fetched body is cut to the cap and says so", () => {
  const r = fetchResult({ status: 200, content_type: "application/json", body_text: "abc", truncated: false });
  assert.deepEqual(r, { status: 200, contentType: "application/json", body: "abc", truncated: false });
  const long = fetchResult({ status: 200, content_type: null, body_text: "x".repeat(1024 * 1024 + 10), truncated: false });
  assert.equal(long.body.length, 1024 * 1024);
  assert.equal(long.truncated, true);
});

test("every refusal the bridge makes reaches the diagnostic log with its code — and an answer reaches it with nothing", async () => {
  const { okReply, refusedCode } = await import("./addonBridgeModel.mjs");
  for (const code of REFUSALS) assert.equal(refusedCode(errorReply("c1", code)), code);
  assert.equal(refusedCode(okReply("c1", { shown: true })), null);
  assert.equal(refusedCode(eventMessage("theme", { scheme: "dark" })), null, "an event is no reply");
  assert.equal(refusedCode(null), null);
  assert.equal(refusedCode({ kind: "bisa:reply", ok: false }), "refused", "a refusal that names no code is one all the same");
  const { readFileSync } = await import("node:fs");
  const bridge = readFileSync(new URL("./addonBridge.ts", import.meta.url), "utf8");
  assert.ok(bridge.includes('if (code) log.info("addons", "a call was refused", { addon: addon.id, method, code });'));
  assert.ok(!/post\(errorReply\(/.test(bridge), "no refusal goes to the frame past the log");
  assert.ok(bridge.includes("if (out.reply) answer(out.reply as Reply, parsed.method);"), "a call's answer is judged as it leaves");
});

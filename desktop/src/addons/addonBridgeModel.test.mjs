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
  MAX_SOLID_RADIUS,
  MAX_SOLID_RECTS,
  MAX_STORAGE_KEY_CHARS,
  METHODS,
  METHOD_NAMES,
  MIN_PLAYER_SIDE,
  NOTIFY_MIN_MS,
  PERMISSIONS,
  PROTOCOL_VERSION,
  REFUSALS,
  STORAGE_QUOTA_BYTES,
  errorReply,
  eventMessage,
  fetchResult,
  frameOf,
  handleCall,
  helloMessage,
  isVideoId,
  mediaGrant,
  networkHosts,
  newSession,
  paramsOf,
  parseAddonMessage,
  permissionWords,
  playerBox,
  refusal,
  refusalWords,
  sceneRect,
  sceneRects,
  storageBag,
  storageBytes,
  storageSet,
  windowAllows,
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
  assert.equal(METHOD_NAMES.length, 23);
  for (const name of METHOD_NAMES) assert.ok(name in METHODS);
  for (const spec of Object.values(METHODS)) assert.ok(spec.permission === null || PERMISSIONS.includes(spec.permission), JSON.stringify(spec));
  assert.equal(METHODS["window.resize"].needs, "resizable");
  assert.equal(METHODS["window.close"].needs, "closable");
  for (const route of ADDON_ROUTES) assert.ok(ROUTE_NAMES.includes(route), route);
  assert.ok(!ADDON_ROUTES.includes("goal") && !ADDON_ROUTES.includes("workbench"), "never a record's page");
  assert.deepEqual([...ADDON_ROUTES], ["inbox", "goals", "pulse", "workflows", "projects", "channels", "messages", "agents", "teams", "settings"], "ten screens; the Triggers screen is gone");
  assert.deepEqual([...EVENTS], ["theme", "system.load", "workspace.summary", "visibility", "media", "scene"]);
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

test("params that cannot be written down are bad, an address that is no URL is refused, the clipboard is written for a granted addon, and a method nobody handles is unknown", () => {
  const unwritable = { v: 1, kind: "bisa:call", id: "c9", method: "storage.set", params: { key: "k", value: 1n } };
  assert.deepEqual(parseAddonMessage(unwritable), { kind: "bad", id: "c9", code: "bad_params" });
  assert.equal(paramsOf("url.open", { url: "http://[" }).ok, false, "a URL the parser throws on");
  assert.equal(paramsOf("network.fetch", { url: "http://[" }).ok, false);
  const s = ready(addon(["clipboard_write"]));
  const written = handleCall(s, call("clipboard.write", { text: "copied" }), ctx());
  assert.deepEqual(written.reply, { v: 1, kind: "bisa:reply", id: "c1", ok: true, result: { written: true } });
  assert.deepEqual(written.effect, { type: "clipboard", text: "copied" });
  assert.equal(handleCall(ready(), call("nobody.handles"), ctx()).reply.error.code, "unknown_method");
});

test("a media grant is hosts and a youtube flag, and the embed needs the flag", () => {
  const any = { media: { hosts: ["*"], youtube: true } };
  const links = { media: { hosts: ["cdn.example"] } };
  assert.deepEqual(mediaGrant([any]), { hosts: ["*"], youtube: true });
  assert.deepEqual(mediaGrant(["storage", links]), { hosts: ["cdn.example"], youtube: false });
  assert.deepEqual(mediaGrant([{ media: { hosts: [7, "a.example"], youtube: "yes" } }]), { hosts: ["a.example"], youtube: false }, "read only as far as its shape allows");
  assert.deepEqual(mediaGrant([{ media: null }]), { hosts: [], youtube: false });
  assert.equal(mediaGrant(["storage"]), null);
  assert.deepEqual(permissionWords([any, "storage"]), ["media", "storage"]);
  assert.ok(PERMISSIONS.includes("media") && METHODS["media.embed"].permission === "media");
  const box = { x: 0, y: 0, width: 240, height: 200 };
  const embed = { kind: "youtube", video: "dQw4w9WgXcQ", box };
  // Not granted at all: `permission`, before the params are read.
  assert.equal(handleCall(ready(addon(["storage"])), call("media.embed", embed), ctx()).reply.error.code, "permission");
  // Hosts alone: the player is not allowed; a box and a removal still answer.
  const hostsOnly = ready(addon([links]));
  assert.equal(handleCall(hostsOnly, call("media.embed", embed), ctx()).reply.error.code, "not_allowed");
  assert.deepEqual(handleCall(hostsOnly, call("media.box", { box }), ctx()).effect, { type: "media_box", box });
  assert.deepEqual(handleCall(hostsOnly, call("media.remove"), ctx()).effect, { type: "media_remove" });
  assert.deepEqual(handleCall(hostsOnly, call("media.remove"), ctx()).reply.result, { removed: true });
  // Granted with the player: the embed is an effect for the host, answered at once.
  const out = handleCall(ready(addon([any])), call("media.embed", embed), ctx());
  assert.deepEqual(out.reply.result, { embedded: true });
  assert.deepEqual(out.effect, { type: "media_embed", video: "dQw4w9WgXcQ", box });
  assert.equal(handleCall(ready(addon([any])), call("media.box", { box }), ctx()).reply.result.moved, true);
  assert.equal(newSession(addon([any])).media.youtube, true);
  assert.equal(newSession(addon()).media, null);
  assert.equal(eventMessage("media", { state: "playing", title: "x" }).topic, "media");
});

test("an embed names an eleven-character video and a box no smaller than the policy's square", () => {
  assert.equal(MIN_PLAYER_SIDE, 200);
  const box = { x: 8, y: 8, width: 320, height: 200 };
  for (const video of ["dQw4w9WgXcQ", "a-b_c-d_e-f"]) assert.ok(isVideoId(video), video);
  for (const video of ["", "dQw4w9WgXc", "dQw4w9WgXcQQ", "dQw4w9WgXc!", "dQw4w9WgXc/", 11, null]) assert.ok(!isVideoId(video), String(video));
  assert.deepEqual(paramsOf("media.embed", { kind: "youtube", video: "dQw4w9WgXcQ", box }), { ok: true, value: { kind: "youtube", video: "dQw4w9WgXcQ", box } });
  assert.deepEqual(paramsOf("media.embed", { kind: "vimeo", video: "dQw4w9WgXcQ", box }), { ok: false, code: "bad_params" });
  assert.deepEqual(paramsOf("media.embed", { kind: "youtube", video: "dQw4w9WgXcQ/x", box }), { ok: false, code: "bad_params" });
  assert.deepEqual(paramsOf("media.embed", { kind: "youtube", video: "dQw4w9WgXcQ" }), { ok: false, code: "bad_params" }, "no box");
  for (const bad of [null, {}, { ...box, width: 199 }, { ...box, height: 199 }, { ...box, x: -1 }, { ...box, y: 1.5 }, { ...box, width: Infinity }, { ...box, height: "200" }]) {
    assert.equal(playerBox(bad), null, JSON.stringify(bad));
    assert.deepEqual(paramsOf("media.box", { box: bad }), { ok: false, code: "bad_params" });
  }
  assert.deepEqual(playerBox({ ...box, extra: 1 }), box, "only the four numbers are kept");
  assert.deepEqual(paramsOf("media.box", { box }), { ok: true, value: { box } });
  assert.deepEqual(paramsOf("media.remove", { anything: 1 }), { ok: true, value: {} });
});

test("a scene's methods need a scene; its session is never resizable and always see-through; its regions are whole and few", () => {
  for (const m of ["scene.mode", "scene.solid", "scene.release"]) assert.equal(METHODS[m].needs, "scene", m);
  assert.equal(frameOf({ frame: "scene" }), "scene");
  assert.equal(frameOf({ frame: "none" }), "none");
  assert.equal(frameOf({}), "bar");
  assert.equal(frameOf(null), "bar");
  assert.equal(frameOf({ frame: "glass" }), "bar", "a word the manifest's rules refuse reads as a bar");
  const bar = ready(addon());
  assert.equal(handleCall(bar, call("scene.mode", { mode: "hunt" }), ctx()).reply.error.code, "not_allowed", "a window is no scene");
  const scene = ready(addon([], { frame: "scene", resizable: true, transparent: false }));
  assert.deepEqual(scene.window, { width: 200, height: 120, resizable: false, closable: true, transparent: true, frame: "scene" });
  assert.equal(handleCall(scene, call("window.resize", { width: 1, height: 1 }), ctx()).reply.error.code, "not_allowed", "a scene is never resized");
  const mode = handleCall(scene, call("scene.mode", { mode: "hunt" }), ctx());
  assert.deepEqual(mode.reply.result, { mode: "hunt" });
  assert.deepEqual(mode.effect, { type: "scene_mode", mode: "hunt" });
  assert.deepEqual(paramsOf("scene.mode", { mode: "fly" }), { ok: false, code: "bad_params" });
  assert.deepEqual(paramsOf("scene.mode", {}), { ok: false, code: "bad_params" });
  const rect = { x: 960, y: 200, width: 56, height: 300 };
  const solid = handleCall(scene, call("scene.solid", { rects: [rect] }), ctx());
  assert.deepEqual(solid.effect, { type: "scene_solid", rects: [{ ...rect, radius: 0 }] });
  assert.deepEqual(solid.reply.result, { set: true });
  assert.deepEqual(handleCall(scene, call("scene.release"), ctx()).effect, { type: "scene_release" });
  assert.deepEqual(handleCall(scene, call("scene.release"), ctx()).reply.result, { released: true });
  assert.deepEqual(sceneRects([]), [], "no solid region is a fine answer");
  assert.equal(MAX_SOLID_RECTS, 8);
  assert.equal(sceneRects(Array(MAX_SOLID_RECTS + 1).fill(rect)), null, "nine is too many");
  assert.equal(sceneRects(Array(MAX_SOLID_RECTS).fill(rect)).length, 8);
  for (const bad of [{ ...rect, width: 0 }, { ...rect, height: 0 }, { ...rect, x: -1 }, { ...rect, y: 1.5 }, { ...rect, radius: -1 }, { ...rect, radius: "8" }, null, "x"]) {
    assert.equal(sceneRect(bad), null, JSON.stringify(bad));
  }
  assert.equal(sceneRects("x"), null);
  assert.equal(sceneRects([rect, { ...rect, width: 0 }]), null, "one bad region refuses the list");
  assert.equal(sceneRect({ ...rect, radius: 999 }).radius, MAX_SOLID_RADIUS, "the corner is capped");
  assert.deepEqual(paramsOf("scene.solid", { rects: "x" }), { ok: false, code: "bad_params" });
  assert.equal(windowAllows(scene.window, "closable"), true);
  assert.equal(windowAllows(scene.window, "resizable"), false);
  assert.equal(windowAllows(bar.window, "scene"), false);
  assert.equal(windowAllows(bar.window, "flying"), false, "a need the registry does not know allows nothing");
  assert.equal(eventMessage("scene", { mode: "work" }).topic, "scene");
});

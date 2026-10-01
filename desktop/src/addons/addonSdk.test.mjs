/**
 * The library an addon runs (`addons/sdk/bisa-addon.js`), driven with a fake
 * window: it says hello to its parent, hears its parent and nobody else,
 * waits for the app's ready before any call, routes replies by id, names
 * exactly the methods and events the bridge model does, and reaches for
 * nothing a sandbox would refuse.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { EVENTS, METHOD_NAMES } from "./addonBridgeModel.mjs";

const SOURCE = readFileSync(new URL("../../../addons/sdk/bisa-addon.js", import.meta.url), "utf8");

/** A window as the frame would have one: a parent that records what it is told, a message listener. */
function fakeWindow() {
  const posted = [];
  const listeners = [];
  const parent = { postMessage: (m) => posted.push(m) };
  const win = {
    parent,
    posted,
    addEventListener: (type, fn) => type === "message" && listeners.push(fn),
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (h) => clearTimeout(h),
    deliver: (data, source = parent) => listeners.forEach((fn) => fn({ data, source })),
  };
  vm.runInNewContext(SOURCE, { window: win, Promise, JSON, Math, Array, Error, Object });
  return win;
}

/** A value made inside the vm, as this realm's plain data. */
const plain = (v) => JSON.parse(JSON.stringify(v));

const ready = (win) => win.deliver({ v: 1, kind: "bisa:ready", addon: { id: "a" }, granted: ["storage"], locale: "en", theme: { scheme: "dark" }, window: { width: 1, height: 1 } });

test("the library says hello to its parent and installs window.bisa", () => {
  const win = fakeWindow();
  assert.deepEqual(plain(win.posted), [{ v: 1, kind: "bisa:hello" }]);
  assert.equal(typeof win.bisa.call, "function");
  assert.equal(win.bisa.version, 1);
});

test("the library names exactly the methods and the events the bridge knows", () => {
  const win = fakeWindow();
  assert.deepEqual(plain(win.bisa.methods()), [...METHOD_NAMES]);
  assert.deepEqual(plain(win.bisa.events()), [...EVENTS]);
});

test("a call waits for ready, then is routed by id, and a foreign source is ignored", async () => {
  const win = fakeWindow();
  const pending = win.bisa.storage.get("city");
  assert.equal(win.posted.length, 1, "nothing is asked before the app is ready");
  win.deliver({ v: 1, kind: "bisa:ready" }, { not: "the parent" });
  assert.equal(win.posted.length, 1, "a ready from a stranger is nothing");
  ready(win);
  await new Promise((r) => setImmediate(r));
  assert.equal(win.posted.length, 2);
  const call = win.posted[1];
  assert.equal(call.kind, "bisa:call");
  assert.equal(call.method, "storage.get");
  assert.deepEqual(plain(call.params), { key: "city" });
  win.deliver({ v: 1, kind: "bisa:reply", id: "nope", ok: true, result: 1 });
  win.deliver({ v: 1, kind: "bisa:reply", id: call.id, ok: true, result: { value: "Lisbon" } }, { not: "the parent" });
  win.deliver({ v: 1, kind: "bisa:reply", id: call.id, ok: true, result: { value: "Lisbon" } });
  assert.deepEqual(plain(await pending), { value: "Lisbon" });
  assert.equal(win.bisa.granted("storage"), true);
  assert.equal(win.bisa.granted("network"), false);
  const hello = await win.bisa.ready();
  assert.equal(hello.theme.scheme, "dark");
});

test("a refusal rejects with its code, an unknown method never leaves, and events reach listeners", async () => {
  const win = fakeWindow();
  ready(win);
  const asked = win.bisa.notify.show({ title: "hi" });
  await new Promise((r) => setImmediate(r));
  const call = win.posted.at(-1);
  win.deliver({ v: 1, kind: "bisa:reply", id: call.id, ok: false, error: { code: "permission", message: "not granted" } });
  await assert.rejects(asked, (e) => e.code === "permission" && e.message === "not granted");
  await assert.rejects(win.bisa.call("fs.read", {}), (e) => e.code === "unknown_method");
  const heard = [];
  const off = win.bisa.on("theme", (p) => heard.push(p));
  win.bisa.on("secrets", () => heard.push("never"));
  win.deliver({ v: 1, kind: "bisa:event", topic: "theme", payload: { scheme: "light" } });
  win.deliver({ v: 1, kind: "bisa:event", topic: "secrets", payload: 1 });
  off();
  win.deliver({ v: 1, kind: "bisa:event", topic: "theme", payload: { scheme: "dark" } });
  assert.deepEqual(plain(heard), [{ scheme: "light" }]);
});

test("the library reaches for nothing a sandbox would refuse", () => {
  for (const forbidden of ["fetch(", "XMLHttpRequest", "localStorage", "sessionStorage", "document.cookie", "indexedDB", "WebSocket", "navigator.sendBeacon", "window.open(", "location.href", "eval(", "new Function"]) {
    assert.ok(!SOURCE.includes(forbidden), `${forbidden} has no place in the library`);
  }
  assert.ok(SOURCE.includes('event.source !== parent'), "only the frame's own parent is heard");
});

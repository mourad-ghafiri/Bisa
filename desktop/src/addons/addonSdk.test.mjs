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

/** A window as the frame would have one: a parent that records what it is told, a message listener — and, asked, timers fired by hand (`win.timers`, `win.fire()`). */
function fakeWindow({ manualTimers = false } = {}) {
  const posted = [];
  const listeners = [];
  const timers = [];
  const parent = { postMessage: (m) => posted.push(m) };
  const win = {
    parent,
    posted,
    timers,
    addEventListener: (type, fn) => type === "message" && listeners.push(fn),
    setTimeout: manualTimers ? (fn, ms) => timers.push({ fn, ms }) : (fn, ms) => setTimeout(fn, ms),
    clearTimeout: manualTimers ? () => {} : (h) => clearTimeout(h),
    /** Fire the oldest timer not yet fired, answering its wait. */
    fire: () => {
      const next = timers.shift();
      next.fn();
      return next.ms;
    },
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

test("the hello is said again at growing waits until ready, then never; past the last, a waiting call fails and a late ready still serves", async () => {
  const win = fakeWindow({ manualTimers: true });
  const hellos = () => win.posted.filter((m) => m.kind === "bisa:hello").length;
  assert.equal(hellos(), 1);
  const early = win.bisa.storage.get("city");
  const readied = win.bisa.ready();
  // Each wait answered without a ready: one more hello, at a longer wait.
  const waits = [];
  for (let i = 0; i < 5; i++) {
    waits.push(win.fire());
    assert.equal(hellos(), i + 2);
  }
  assert.deepEqual(waits, [250, 500, 1000, 2000, 4000]);
  assert.equal(win.timers.length, 0, "after the last wait, no more hellos are scheduled");
  // The waiting call fails at once; so does one made now; ready() keeps waiting.
  await assert.rejects(early, (e) => e.code === "unavailable" && /did not answer storage\.get/.test(e.message));
  await assert.rejects(win.bisa.theme.get(), (e) => e.code === "unavailable");
  assert.equal(win.posted.filter((m) => m.kind === "bisa:call").length, 0, "nothing is asked of a platform that never answered");
  // A ready that still arrives serves ready() and every call made after.
  ready(win);
  const hello = await readied;
  assert.equal(hello.theme.scheme, "dark");
  const late = win.bisa.storage.get("city");
  const call = win.posted.at(-1);
  assert.equal(call.kind, "bisa:call");
  win.deliver({ v: 1, kind: "bisa:reply", id: call.id, ok: true, result: { value: "Lisbon" } });
  assert.deepEqual(plain(await late), { value: "Lisbon" });
  assert.equal(hellos(), 6, "ready heard: the hello is never said again");
});

test("a ready that comes after the first wait sends what was queued, in order, and stops the hellos", () => {
  const win = fakeWindow({ manualTimers: true });
  const first = win.bisa.storage.get("a");
  const second = win.bisa.storage.keys();
  win.fire();
  assert.equal(win.posted.filter((m) => m.kind === "bisa:hello").length, 2);
  ready(win);
  const calls = win.posted.filter((m) => m.kind === "bisa:call");
  assert.deepEqual(calls.map((c) => c.method), ["storage.get", "storage.keys"], "queued calls go out in the order they were made");
  win.fire();
  assert.equal(win.posted.filter((m) => m.kind === "bisa:hello").length, 2, "a wait that ends after ready says nothing");
  void first;
  void second;
});

test("the library reaches for nothing a sandbox would refuse", () => {
  for (const forbidden of ["fetch(", "XMLHttpRequest", "localStorage", "sessionStorage", "document.cookie", "indexedDB", "WebSocket", "navigator.sendBeacon", "window.open(", "location.href", "eval(", "new Function"]) {
    assert.ok(!SOURCE.includes(forbidden), `${forbidden} has no place in the library`);
  }
  assert.ok(SOURCE.includes('event.source !== parent'), "only the frame's own parent is heard");
});

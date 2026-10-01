/**
 * What an agent's browser request means for the tabs. Run with
 * `node --test desktop/src/shell/browserBridgeModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { EMPTY_BROWSERS, PERSON, openBrowser, titled } from "./browsersModel.mjs";
import { DEFAULT_WAIT_MS, LOAD_LATE, MAX_WAIT_MS, NAVIGATING, NOT_SHOWN, PAGE_SILENT, PLACEHOLDER_ID, PRESENCE_LAPSE_AFTER, SHOT_FAILED, TAB_GONE, answerResult, askAgain, presenceLapsed, navigatedResult, planRequest, revealPlan, rootFor, screenshotResult, tabResult, tabsResult, waitBound, wireResult } from "./browserBridgeModel.mjs";

const here = { scope: "workstream", id: "w9" };
let state = openBrowser(EMPTY_BROWSERS, { by: PERSON, home: { scope: "workstream", id: "w1" }, url: "http://localhost:5173/" });
state = titled(state, "b1", "Home");
state = openBrowser(state, { by: PERSON, home: { scope: "workstream", id: "w1" }, url: "https://example.com/" });
state = openBrowser(state, { by: PERSON, home: { scope: "workstream", id: "w1" } });
const home = (scope, id) => ({ home: { scope, id } });

test("a new tab is at home where the engine said the session speaks — any of the desktop's scopes — else where the IDE is, else the workspace", () => {
  assert.deepEqual(rootFor(home("workstream", "w1"), here), { scope: "workstream", id: "w1" });
  assert.deepEqual(rootFor(home("goal", "g1"), here), { scope: "goal", id: "g1" }, "a goal's conversation: beside the goal");
  assert.deepEqual(rootFor(home("channel", "general"), here), { scope: "channel", id: "general" }, "a channel's turn: beside the channel");
  assert.deepEqual(rootFor(home("dm", "dm-01abc"), null), { scope: "dm", id: "dm-01abc" }, "a direct message's turn: beside the message");
  assert.deepEqual(rootFor(home("workflow", "f1"), here), { scope: "workflow", id: "f1" });
  assert.deepEqual(rootFor(home("work_item", "i1"), here), { scope: "work_item", id: "i1" });
  assert.deepEqual(rootFor(home("conversation", "c1"), here), { scope: "conversation", id: "c1" }, "… wherever the IDE is");
  assert.deepEqual(rootFor({}, here), here, "no home: the root the IDE is on");
  assert.equal(rootFor({}, null), null, "no IDE, no home: the workspace's tab");
  assert.deepEqual(rootFor(home("machine", "x"), here), here, "a scope the desktop cannot show a tab beside is no home");
  assert.deepEqual(rootFor({ home: { scope: "goal", id: "" } }, here), here, "a home with no id is none");
  assert.equal(rootFor(null, null), null);
  const open = planRequest({ action: "open", url: "localhost:4173" }, home("workstream", "w1"), state, here);
  assert.deepEqual(open, { kind: "open", url: "http://localhost:4173/", tab: null, home: { scope: "workstream", id: "w1" }, headless: false }, "a bare host and port is a page on this machine");
  assert.equal(planRequest({ action: "open", url: "file:///x" }, {}, state, here).kind, "refuse");
  assert.deepEqual(planRequest({ action: "open", url: "http://x/" }, {}, state, null), { kind: "open", url: "http://x/", tab: null, home: null, headless: false }, "never refused for want of a root");
  const into = planRequest({ action: "open", url: "http://localhost:5173/pricing", tab: "b1" }, {}, state, here);
  assert.equal(into.kind, "open");
  assert.equal(into.tab, "b1", "navigating a tab the agent already has");
  assert.match(planRequest({ action: "open", url: "http://x/", tab: "b9" }, {}, state, here).error, /no tab b9/);
});

test("reading, outlining and acting reach any page the tab shows — this machine's or the web's — by a ref or a selector, and nothing on a blank tab", () => {
  const drive = (request) => planRequest(request, {}, state, here);
  assert.deepEqual(drive({ action: "read", tab: "b1" }), { kind: "drive", key: "b1", message: { type: "bisa:read", id: PLACEHOLDER_ID, target: null, format: "text" }, navigates: false });
  assert.deepEqual(drive({ action: "read", tab: "b1", target: "e3", format: "html" }).message, { type: "bisa:read", id: PLACEHOLDER_ID, target: "e3", format: "html" });
  assert.deepEqual(drive({ action: "snapshot", tab: "b1" }).message, { type: "bisa:snapshot", id: PLACEHOLDER_ID, target: null, all: false });
  assert.deepEqual(drive({ action: "snapshot", tab: "b1", target: "main", all: true }).message, { type: "bisa:snapshot", id: PLACEHOLDER_ID, target: "main", all: true });
  assert.deepEqual(drive({ action: "find", tab: "b1", query: "price" }).message, { type: "bisa:find-text", id: PLACEHOLDER_ID, query: "price" });
  assert.match(drive({ action: "find", tab: "b1", query: " " }).error, /needs words/);
  const click = drive({ action: "click", tab: "b1", target: "#buy" });
  assert.deepEqual(click, { kind: "drive", key: "b1", message: { type: "bisa:click", id: PLACEHOLDER_ID, target: "#buy" }, navigates: true }, "a click may move the page");
  assert.deepEqual(drive({ action: "fill", tab: "b1", target: "e4", text: "hats" }).message, { type: "bisa:fill", id: PLACEHOLDER_ID, target: "e4", text: "hats" });
  assert.deepEqual(drive({ action: "type", tab: "b1", target: "e4", text: "hats", clear: true, submit: true }).message, { type: "bisa:type", id: PLACEHOLDER_ID, target: "e4", text: "hats", clear: true, submit: true });
  assert.deepEqual(drive({ action: "press", tab: "b1", key: "Enter", modifiers: ["shift"] }).message, { type: "bisa:press", id: PLACEHOLDER_ID, key: "Enter", target: null, modifiers: ["shift"] });
  assert.match(drive({ action: "press", tab: "b1", key: " " }).error, /needs a key/);
  assert.deepEqual(drive({ action: "select", tab: "b1", target: "#region", value: "eu" }).message, { type: "bisa:select", id: PLACEHOLDER_ID, target: "#region", value: "eu", label: null });
  assert.match(drive({ action: "select", tab: "b1", target: "#region" }).error, /needs the option's value or its label/);
  assert.deepEqual(drive({ action: "hover", tab: "b1", target: "e2" }), { kind: "drive", key: "b1", message: { type: "bisa:point", id: PLACEHOLDER_ID, target: "e2" }, navigates: false });
  assert.deepEqual(drive({ action: "scroll", tab: "b1", to: "bottom" }).message, { type: "bisa:scroll", id: PLACEHOLDER_ID, to: "bottom", byX: 0, byY: 0 });
  assert.deepEqual(drive({ action: "scroll", tab: "b1", by_y: 600 }).message, { type: "bisa:scroll", id: PLACEHOLDER_ID, to: null, byX: 0, byY: 600 });
  assert.deepEqual(drive({ action: "console", tab: "b1", clear_console: true }).message, { type: "bisa:console", id: PLACEHOLDER_ID, clear: true });
  assert.deepEqual(drive({ action: "eval", tab: "b1", expression: "1 + 1" }).message, { type: "bisa:eval", id: PLACEHOLDER_ID, expression: "1 + 1" });
  assert.match(drive({ action: "eval", tab: "b1", expression: " " }).error, /needs an expression/);
  for (const action of ["click", "fill", "type", "select", "hover"]) {
    assert.match(drive({ action, tab: "b1" }).error, new RegExp(`browser_${action} needs a target`), `${action} without a target`);
  }
  assert.deepEqual([...NAVIGATING], ["click", "fill", "type", "press", "select"]);
  for (const action of NAVIGATING) assert.equal(drive({ action, tab: "b1", target: "#a", key: "Enter", value: "x" }).navigates, true, `${action} may move the page`);
  assert.deepEqual(drive({ action: "read", tab: "b2" }).message.type, "bisa:read", "the web is read like any page");
  assert.match(drive({ action: "read", tab: "b3" }).error, /no page yet/, "a blank tab has nothing to read");
  assert.match(drive({ action: "read" }).error, /name a tab/);
  assert.match(drive({ action: "read", tab: "b7" }).error, /no tab b7/);
  assert.deepEqual(drive({ action: "back", tab: "b1" }), { kind: "back", key: "b1" });
  assert.deepEqual(drive({ action: "forward", tab: "b1" }), { kind: "forward", key: "b1" });
  assert.deepEqual(drive({ action: "reload", tab: "b1" }), { kind: "reload", key: "b1" });
  assert.match(drive({ action: "reload", tab: "b3" }).error, /no page yet/, "nothing to load again on a blank tab");
  assert.deepEqual(drive({ action: "close", tab: "b1" }), { kind: "close", key: "b1" });
  assert.deepEqual(drive({ action: "tabs" }), { kind: "tabs" });
  assert.match(drive({ action: "wander" }).error, /unknown browser action/);
});

test("a wait for the load is the bridge's own, bounded; a wait for an element, some words, its going or quiet is the page's", () => {
  const drive = (request) => planRequest(request, {}, state, here);
  assert.deepEqual(drive({ action: "wait", tab: "b1" }), { kind: "wait-load", key: "b1", timeoutMs: DEFAULT_WAIT_MS }, "until load by default");
  assert.deepEqual(drive({ action: "wait", tab: "b1", until: "load", timeout_ms: 90_000 }), { kind: "wait-load", key: "b1", timeoutMs: MAX_WAIT_MS }, "held to the most");
  assert.deepEqual(drive({ action: "wait", tab: "b3", until: "load" }), { kind: "wait-load", key: "b3", timeoutMs: DEFAULT_WAIT_MS }, "a blank tab may be waited for");
  assert.deepEqual(drive({ action: "wait", tab: "b1", until: "selector", target: "#done", timeout_ms: 5000 }).message, { type: "bisa:wait", id: PLACEHOLDER_ID, until: "selector", target: "#done", query: null, timeoutMs: 5000 });
  assert.deepEqual(drive({ action: "wait", tab: "b1", until: "text", query: "Signed in" }).message, { type: "bisa:wait", id: PLACEHOLDER_ID, until: "text", target: null, query: "Signed in", timeoutMs: DEFAULT_WAIT_MS });
  assert.deepEqual(drive({ action: "wait", tab: "b1", until: "gone", target: "e9" }).message.until, "gone");
  assert.deepEqual(drive({ action: "wait", tab: "b1", until: "idle" }).message, { type: "bisa:wait", id: PLACEHOLDER_ID, until: "idle", target: null, query: null, timeoutMs: DEFAULT_WAIT_MS });
  assert.match(drive({ action: "wait", tab: "b1", until: "selector" }).error, /needs a target/);
  assert.match(drive({ action: "wait", tab: "b1", until: "text" }).error, /needs the words/);
  assert.match(drive({ action: "wait", tab: "b1", until: "forever" }).error, /load, selector, text, gone or idle/);
  assert.match(drive({ action: "wait", tab: "b9" }).error, /no tab b9/);
  assert.equal(waitBound(undefined), DEFAULT_WAIT_MS);
  assert.equal(waitBound(-5), DEFAULT_WAIT_MS);
  assert.equal(waitBound(250.7), 250);
  assert.equal(waitBound(1e9), MAX_WAIT_MS);
  assert.match(LOAD_LATE, /still loading/);
});

test("a screenshot is of a tab with a page, on any page; the tab is shown in the IDE when it is on the tab's home and its centre shows documents, else in the pane", () => {
  assert.deepEqual(planRequest({ action: "screenshot", tab: "b1" }, {}, state, here), { kind: "screenshot", key: "b1" });
  assert.deepEqual(planRequest({ action: "screenshot", tab: "b2" }, {}, state, here), { kind: "screenshot", key: "b2" }, "the web too: pixels need no door into the page");
  assert.match(planRequest({ action: "screenshot", tab: "b3" }, {}, state, here).error, /no page yet/);
  assert.match(planRequest({ action: "screenshot" }, {}, state, here).error, /name a tab/);
  assert.match(planRequest({ action: "screenshot", tab: "b9" }, {}, state, here).error, /no tab b9/);
  const tab = state.sessions[0];
  const home = { scope: "workstream", id: "w1" };
  assert.equal(revealPlan(tab, home, "documents"), "ide", "at home, the strip in Project Mode");
  assert.equal(revealPlan(tab, home, "conversation"), "pane", "at home, but the centre is the conversation: the pane shows it — as the Goal and Workflow screens do");
  assert.equal(revealPlan(tab, home, "board"), "pane", "the Board too");
  assert.equal(revealPlan(tab, here, "documents"), "pane", "the IDE is on another root: the pane shows it");
  assert.equal(revealPlan(tab, null, "documents"), "pane");
  assert.equal(revealPlan({ ...tab, home: null }, here, "documents"), "pane", "the workspace's tab is the pane's");
  assert.equal(revealPlan({ ...tab, headless: true }, home, "documents"), null, "a tab kept out of sight is never revealed by an agent's act");
  assert.equal(revealPlan({ ...tab, headless: true }, home, "conversation"), null, "whatever the centre");
  assert.equal(planRequest({ action: "open", url: "http://localhost:5173/" }, {}, state, here, true).headless, true, "the engine's word rides into the open");
  assert.equal(planRequest({ action: "open", url: "http://localhost:5173/" }, {}, state, here).headless, false);
  const shot = { sha256: "a".repeat(64), name: "browser-b1-01X.png", mime: "image/png", size: 10 };
  assert.deepEqual(screenshotResult(tab, shot, { width: 1280, height: 800 }), { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", screenshot: shot, width: 1280, height: 800 });
  assert.match(NOT_SHOWN, /could not be shown/);
  assert.match(SHOT_FAILED, /could not be taken/);
});

test("the answers the agent reads: the tabs without the blank one, a tab as it stands, a move said so, and the page's own words with every fact it gave", () => {
  assert.deepEqual(tabsResult(state.sessions), { ok: true, tabs: [{ key: "b1", url: "http://localhost:5173/", title: "Home" }, { key: "b2", url: "https://example.com/", title: "" }] });
  assert.deepEqual(tabResult(state.sessions[0]), { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home" });
  assert.deepEqual(navigatedResult(state.sessions[0]), { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", navigated: true });
  assert.deepEqual(answerResult(state.sessions[0], { ok: true, text: "Hello", url: "http://localhost:5173/", title: "Home" }), { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", text: "Hello" }, "nothing extra when the page gave nothing extra");
  const dialogs = [{ kind: "confirm", message: "Sure?", answer: "true" }];
  const console = [{ level: "error", text: "boom", at: 12 }];
  const scroll = { x: 0, y: 1200, width: 1280, height: 4800 };
  assert.deepEqual(
    answerResult(state.sessions[0], { ok: true, text: null, url: "http://localhost:5173/", title: "Home", selector: "#go", count: 7, waitedMs: 340, scroll, value: { n: 1 }, dialogs, console }),
    { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", text: null, count: 7, waited_ms: 340, scroll, value: { n: 1 }, dialogs, console },
    "the element's path is the inspector's own: the wire has no key for it",
  );
  assert.deepEqual(answerResult(state.sessions[0], { ok: false, error: "nothing matches #x", dialogs }), { ok: false, error: "nothing matches #x", tab: "b1", url: "http://localhost:5173/", title: "Home", dialogs }, "a refusal still carries the dialogs the page raised");
  assert.deepEqual(answerResult(state.sessions[0], { ok: true, value: 0, dialogs: [], console: [] }), { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", text: null, value: 0 }, "a zero is a value; empty lists are left out");
  assert.match(PAGE_SILENT, /did not answer/);
});

test("what leaves for the node is the wire's answer, key by key: nothing the type does not declare, nothing absent, the nested facts by their own keys", () => {
  const tab = state.sessions[0];
  const dialogs = [{ kind: "confirm", message: "Sure?", answer: "true" }];
  const console = [{ level: "error", text: "boom", at: 12 }];
  const scroll = { x: 0, y: 1200, width: 1280, height: 4800 };
  const shot = { sha256: "a".repeat(64), name: "browser-b1-01X.png", mime: "image/png", size: 10 };
  // The ordinary answers, as the bridge assembles them.
  assert.deepEqual(wireResult({ ...answerResult(tab, { ok: true, text: "Hello", url: "http://localhost:5173/", title: "Home" }), tabs: [] }), { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", text: "Hello" });
  assert.deepEqual(
    wireResult({ ...answerResult(tab, { ok: true, text: null, selector: "#go", count: 7, waitedMs: 340, scroll, value: { n: 1 }, dialogs, console }), tabs: [] }),
    { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", count: 7, waited_ms: 340, scroll, value: { n: 1 }, dialogs, console },
    "a text that is none and a list that is empty are left out",
  );
  assert.deepEqual(wireResult({ ...tabsResult(state.sessions) }), { ok: true, tabs: [{ key: "b1", url: "http://localhost:5173/", title: "Home" }, { key: "b2", url: "https://example.com/", title: "" }] });
  assert.deepEqual(wireResult({ ...navigatedResult(tab), tabs: [] }), { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", navigated: true });
  assert.deepEqual(wireResult({ ...screenshotResult(tab, shot, { width: 1280, height: 800 }), tabs: [] }), { ok: true, tab: "b1", url: "http://localhost:5173/", title: "Home", width: 1280, height: 800, screenshot: shot });
  assert.deepEqual(wireResult({ ok: false, error: PAGE_SILENT, tabs: [], tab: "b1", url: "http://localhost:5173/" }), { ok: false, error: PAGE_SILENT, tab: "b1", url: "http://localhost:5173/" });
  assert.deepEqual(wireResult({ ok: true, tab: "b1", value: 0 }), { ok: true, tab: "b1", value: 0 }, "a zero is a value");
  // An answer that carries what the wire does not know: every such key stays here.
  assert.deepEqual(
    wireResult({
      ok: true,
      tab: "b1",
      url: "http://localhost:5173/",
      title: "Home",
      selector: "#go",
      path: "/tmp/shot.png",
      session: tab,
      loading: false,
      tabs: [{ key: "b1", url: "http://localhost:5173/", title: "Home", home: { scope: "workstream", id: "w1" }, headless: false }],
      scroll: { ...scroll, top: 0 },
      dialogs: [{ ...dialogs[0], at: 3 }, { kind: "alert", message: "Hi", answer: null }],
      console: [{ ...console[0], source: "page.js" }],
      screenshot: { ...shot, preview: "blob:1" },
    }),
    {
      ok: true,
      tab: "b1",
      url: "http://localhost:5173/",
      title: "Home",
      tabs: [{ key: "b1", url: "http://localhost:5173/", title: "Home" }],
      scroll,
      dialogs: [dialogs[0], { kind: "alert", message: "Hi" }],
      console,
      screenshot: shot,
    },
    "the path is the engine's to set, and never the desktop's",
  );
  // What is no such fact is left out, never sent in a shape the node would refuse whole.
  assert.deepEqual(wireResult({ ok: true, count: 1.5, waited_ms: -1, width: "1280", scroll: { x: 0.5, y: 0, width: 1, height: 1 }, tabs: [{ key: "b1" }, null], dialogs: [{ kind: "alert" }], console: ["boom"], screenshot: { name: "x.png" }, navigated: "yes" }), { ok: true });
  assert.deepEqual(wireResult(null), { ok: false });
  assert.deepEqual(wireResult({ ok: "yes" }), { ok: false }, "only a true is a yes");
});

test("a silent page is asked once more only for an act that cannot move the page, and never a third time", () => {
  const read = { navigates: false };
  const click = { navigates: true };
  assert.equal(askAgain(read, { kind: "silent" }, 1), true, "a silent read is asked again");
  assert.equal(askAgain(read, { kind: "silent" }, 2), false, "and never a third time");
  assert.equal(askAgain(click, { kind: "silent" }, 1), false, "a second click is a different act");
  assert.equal(askAgain(read, { kind: "gone" }, 1), false, "a gone tab is not silence");
  assert.equal(askAgain(read, { kind: "answered" }, 1), false);
});

test("a gone tab and a silent page are two sentences, each saying what to do", () => {
  assert.notEqual(TAB_GONE, PAGE_SILENT);
  assert.ok(TAB_GONE.includes("browser_tabs") && TAB_GONE.includes("browser_open"), "a gone tab names the way back");
  assert.ok(PAGE_SILENT.includes("try again"));
  for (const s of [TAB_GONE, PAGE_SILENT, LOAD_LATE, NOT_SHOWN, SHOT_FAILED]) assert.ok(!s.includes("\n") && s === s.trim(), "one line each");
});

test("the presence read's second miss in a row is the warning, not the first", () => {
  assert.equal(PRESENCE_LAPSE_AFTER, 2);
  assert.equal(presenceLapsed(1), false);
  assert.equal(presenceLapsed(2), true);
  assert.equal(presenceLapsed(5), true);
});

test("the bridge remembers only the latest requests it took up, and a tab nobody waits on holds no row", async () => {
  const { readFileSync } = await import("node:fs");
  const { HANDLED_KEPT } = await import("./browserBridgeModel.mjs");
  const { Lru } = await import("./lru.mjs");
  const bridge = readFileSync(new URL("./browserBridge.ts", import.meta.url), "utf8");
  assert.ok(bridge.includes("const handled = new Lru<true>(HANDLED_KEPT);") && !bridge.includes("new Set<string>()"), "never every request since the window opened");
  const kept = new Lru(HANDLED_KEPT);
  for (let i = 0; i < HANDLED_KEPT + 10; i++) kept.set(`r${i}`, true);
  assert.equal(kept.has("r0"), false);
  assert.equal(kept.has("r10"), true, "a request still on the parked list is never performed twice");
  assert.ok(bridge.includes("else loads.delete(key);") && bridge.includes("else starts.delete(key);"), "a closed tab's key is not kept for good");
});

/**
 * A screen's Browser door. Run with `node --test desktop/src/shell/browserDoorModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { busyWords, doorMenu, doorWords } from "./browserDoorModel.mjs";

const tab = (key, label, home, busy = false, headless = false) => ({ key, label, home, where: home ? `${home.scope === "goal" ? "Goal" : "Project IDE"} · ${home.id === "w1" ? "Bisa › feat/a" : "Ship it"}` : "Workspace", busy, headless });
const goal = { scope: "goal", id: "g1" };
const ids = (items) => items.map((i) => i.id);

test("the button says what is open, whether the pane shows, and when an agent is browsing", () => {
  assert.equal(doorWords({ count: 0, busy: 0, showing: false }).hint, "The embedded browser beside this screen — agents' tabs and yours (⌘⇧L)");
  assert.equal(doorWords({ count: 2, busy: 0, showing: false }).hint, "Show the Browser pane — 2 tabs open (⌘⇧L)");
  assert.equal(doorWords({ count: 1, busy: 0, showing: true }).hint, "Hide the Browser pane (⌘⇧L)");
  assert.equal(doorWords({ count: 1, busy: 1, showing: true }).hint, "An agent is browsing — 1 tab open (⌘⇧L)", "an agent at work is said before anything else");
  assert.deepEqual(doorWords({ count: 3, busy: 0, showing: false }).label, "Browser");
  assert.equal(doorWords({ count: 3, busy: 0, showing: false }).count, 3);
});

test("the menu lists the tabs at home here first, the active one leading, the rest with their home, then a new tab, the pane's close and every tab's", () => {
  const tabs = [tab("b1", "Docs", { scope: "workstream", id: "w1" }), tab("b2", "Pricing", goal), tab("b3", "Checkout", goal, true)];
  const items = doorMenu({ tabs, here: goal, active: "b3", showing: true });
  assert.deepEqual(ids(items), ["tab:b3", "tab:b2", "tab:b1", "new", "hide", "close-all"]);
  assert.equal(items[0].label, "Checkout — an agent is browsing", "a busy tab says so");
  assert.equal(items[1].label, "Pricing", "a tab at home here needs no address");
  assert.equal(items[2].label, "Docs · Project IDE · Bisa › feat/a", "a tab from elsewhere says where it is at home, by name");
  assert.ok(items[0].separatorBefore && items[2].separatorBefore && items[3].separatorBefore && items[4].separatorBefore, "a rule at each group's start");
  assert.ok(!items[1].separatorBefore);
  assert.equal(items[5].label, "Close every tab (3)");
  assert.ok(items[5].danger && !items[5].separatorBefore, "the closes stand together");
});

test("a tab kept out of sight reads unseen with the hidden glyph, wherever it is at home, and a pick is the door that shows it", () => {
  const items = doorMenu({ tabs: [tab("b1", "Admin", goal, false, true), tab("b2", "Docs", { scope: "workstream", id: "w1" }, true, true)], here: goal, active: null, showing: false });
  assert.equal(items[0].label, "Admin · unseen");
  assert.equal(items[0].icon, "hidden");
  assert.equal(items[1].label, "Docs · Project IDE · Bisa › feat/a · unseen — an agent is browsing", "its home, then unseen, then the work");
  assert.equal(items[0].id, "tab:b1", "the same row a seen tab has: picking it opens the pane on it, which shows it");
});

test("hidden, no pane to close; no tab, nothing to close — a new tab alone; one tab is closed by name", () => {
  const hidden = doorMenu({ tabs: [tab("b1", "Docs", null)], here: null, active: "b1", showing: false });
  assert.deepEqual(ids(hidden), ["tab:b1", "new", "close-all"]);
  assert.equal(hidden[2].label, "Close the tab");
  assert.ok(hidden[2].separatorBefore, "with no pane's close, the tab's close starts its own group");
  assert.equal(hidden[0].label, "Docs · Workspace", "no screen place: every tab says its home");
  const none = doorMenu({ tabs: [], here: goal, active: null, showing: false });
  assert.deepEqual(ids(none), ["new"]);
  assert.ok(!none[0].separatorBefore, "the first item wears no rule");
});

test("the footer's word counts the tabs an agent works in", () => {
  assert.equal(busyWords(1), "An agent is browsing in 1 tab");
  assert.equal(busyWords(2), "An agent is browsing in 2 tabs");
});

/**
 * The sidebar's order: the person's, made whole against the destinations
 * that exist. Run with `node --test desktop/src/shell/navOrderModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { NAV_ORDER_KEY, homeKey, isDefaultOrder, orderKeys, orderedNav, placeKey } from "./navOrderModel.mjs";

const DEFAULTS = ["inbox", "agents", "teams", "projects", "workflows", "goals", "pulse"];

test("nothing stored, or garbage, is the file's order", () => {
  assert.deepEqual(orderKeys(null, DEFAULTS), DEFAULTS);
  assert.deepEqual(orderKeys(undefined, DEFAULTS), DEFAULTS);
  assert.deepEqual(orderKeys("pulse,inbox", DEFAULTS), DEFAULTS, "a string is not an order");
  assert.deepEqual(orderKeys({ inbox: 0 }, DEFAULTS), DEFAULTS, "nor an object");
  assert.deepEqual(orderKeys([1, null, {}], DEFAULTS), DEFAULTS, "nor an array of anything else");
  assert.equal(NAV_ORDER_KEY, "bisa.sidebar.order");
});

test("a stored order is kept whole", () => {
  const mine = ["pulse", "goals", "workflows", "projects", "teams", "agents", "inbox"];
  assert.deepEqual(orderKeys(mine, DEFAULTS), mine);
});

test("a stranger is dropped, a missing destination is appended in the file's order, a repeat counts once", () => {
  assert.deepEqual(orderKeys(["pulse", "catalog", "inbox"], DEFAULTS), ["pulse", "inbox", "agents", "teams", "projects", "workflows", "goals"]);
  assert.deepEqual(orderKeys(["teams", "teams", "goals"], DEFAULTS), ["teams", "goals", "inbox", "agents", "projects", "workflows", "pulse"]);
  assert.deepEqual(orderKeys([], DEFAULTS), DEFAULTS);
});

test("an order stored while Triggers was a destination drops it when read, and the rest keeps the person's order", () => {
  const before = ["pulse", "triggers", "goals", "workflows", "projects", "teams", "agents", "inbox"];
  assert.deepEqual(orderKeys(before, DEFAULTS), ["pulse", "goals", "workflows", "projects", "teams", "agents", "inbox"]);
  assert.equal(homeKey(orderKeys(["triggers", "inbox"], DEFAULTS), DEFAULTS), "inbox", "a home that was Triggers falls to the next one kept");
  assert.equal(homeKey(orderKeys(["triggers"], DEFAULTS), DEFAULTS), "inbox", "and to the file's first when nothing else was stored");
});

test("a key is placed at an index, clamped to the ends, and nothing moves for nothing", () => {
  assert.deepEqual(placeKey(DEFAULTS, "pulse", 0), ["pulse", "inbox", "agents", "teams", "projects", "workflows", "goals"]);
  assert.deepEqual(placeKey(DEFAULTS, "inbox", 6), ["agents", "teams", "projects", "workflows", "goals", "pulse", "inbox"]);
  assert.deepEqual(placeKey(DEFAULTS, "inbox", 99), ["agents", "teams", "projects", "workflows", "goals", "pulse", "inbox"], "past the end is the end");
  assert.deepEqual(placeKey(DEFAULTS, "goals", -3), ["goals", "inbox", "agents", "teams", "projects", "workflows", "pulse"], "before the front is the front");
  assert.equal(placeKey(DEFAULTS, "agents", 1), DEFAULTS, "onto itself: the same array, so a store can skip the write");
  assert.equal(placeKey(DEFAULTS, "catalog", 2), DEFAULTS, "a key the order does not hold: the same array");
});

test("entries follow the order; one the order does not name keeps its place after the named", () => {
  const nav = DEFAULTS.map((key) => ({ key, label: key }));
  assert.deepEqual(orderedNav(nav, ["pulse", "inbox"]).map((e) => e.key), ["pulse", "inbox", "agents", "teams", "projects", "workflows", "goals"]);
  assert.deepEqual(orderedNav(nav, []).map((e) => e.key), DEFAULTS);
  assert.deepEqual(orderedNav(nav, DEFAULTS).map((e) => e.key), DEFAULTS);
});

test("the app opens on the first destination of the person's order — the Inbox until something is put above it", () => {
  assert.equal(homeKey(DEFAULTS, DEFAULTS), "inbox");
  assert.equal(homeKey(["pulse", "inbox", "agents", "teams", "projects", "workflows", "goals"], DEFAULTS), "pulse");
  assert.equal(homeKey([], DEFAULTS), "inbox", "nothing ordered: the file's first");
});

test("the default order is known as such", () => {
  assert.equal(isDefaultOrder(DEFAULTS, DEFAULTS), true);
  assert.equal(isDefaultOrder([...DEFAULTS].reverse(), DEFAULTS), false);
  assert.equal(isDefaultOrder(DEFAULTS.slice(1), DEFAULTS), false);
});

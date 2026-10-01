/**
 * The router's facts: every hash is a screen, every screen a hash, and the
 * two are inverses. Run with `node --test desktop/src/routeModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { ROUTE_NAMES, ROUTE_TABLE, WORKBENCH_SCOPES, applySearchPatch, href, indexRouteOf, parse, queryOf, section, splitHash } from "./routeModel.mjs";

/** The home a caller hands `parse` — the router's is the sidebar order's first; here anything but the Inbox, to prove it is the caller's. */
const HOME = Object.freeze({ name: "pulse" });

/** One route of every shape the table names. */
const EVERY_ROUTE = [
  { name: "pulse" },
  { name: "inbox" },
  { name: "goals" },
  { name: "goal", id: "01G" },
  { name: "workflows" },
  { name: "workflow", id: "01W" },
  { name: "run", id: "01R" },
  { name: "projects" },
  { name: "workbench", scope: "workstream", id: "01S" },
  { name: "workbench", scope: "goal", id: "01G" },
  { name: "channels" },
  { name: "channel", id: "general" },
  { name: "messages" },
  { name: "dm", id: "01D" },
  { name: "hosted_channel", host: "ab".repeat(32), id: "general" },
  { name: "hosted_dm", host: "cd".repeat(32), id: "01D" },
  { name: "conversation", id: "01C" },
  { name: "agents" },
  { name: "agent", id: "general-agent" },
  { name: "teams" },
  { name: "settings" },
];

test("parse and href are inverses for every route, and every name in the table is drawn by one", () => {
  for (const route of EVERY_ROUTE) {
    assert.deepEqual(parse(href(route), HOME), route, `${href(route)} round-trips`);
  }
  assert.deepEqual([...new Set(EVERY_ROUTE.map((r) => r.name))].sort(), [...ROUTE_NAMES].sort(), "the fixture covers the table");
  assert.equal(ROUTE_TABLE.length, 20, "ten index screens, ten detail screens");
});

test("the Triggers screen is gone: its hash is no place, and lands on the caller's home", () => {
  assert.ok(!ROUTE_NAMES.includes("triggers"), "no route is named triggers");
  assert.deepEqual(parse("#/triggers", HOME), HOME, "an old link to the Triggers screen opens the home");
  assert.deepEqual(parse("#/triggers?trigger=01T", HOME), HOME);
});

test("a hash the table does not name lands on the caller's home; the bare and empty hashes too; a workbench scope the node has no tree for lands on the index", () => {
  assert.deepEqual(parse("", HOME), HOME, "a launch has no hash: the home");
  assert.deepEqual(parse("#", HOME), HOME);
  assert.deepEqual(parse("#/", HOME), HOME);
  assert.deepEqual(parse("#/nowhere", HOME), HOME);
  assert.deepEqual(parse("#/goals/01G/extra", HOME), HOME, "a segment too many is not the goal");
  assert.deepEqual(parse("", { name: "goals" }), { name: "goals" }, "the home is whatever the caller says, never the Inbox by name");
  assert.deepEqual(parse("#/projects/nonsense/01S", HOME), { name: "projects" });
  for (const scope of WORKBENCH_SCOPES) assert.deepEqual(parse(`#/projects/${scope}/x`, HOME), { name: "workbench", scope, id: "x" });
  assert.notEqual(parse("", HOME), HOME, "a fresh object each time — a caller may hold one");
  assert.notEqual(parse("#/inbox", HOME), HOME);
});

test("an id is carried escaped and read back whole; a malformed escape is not a place", () => {
  const spaced = { name: "channel", id: "a b/c?d" };
  assert.equal(href(spaced), "#/channels/a%20b%2Fc%3Fd");
  assert.deepEqual(parse(href(spaced), HOME), spaced);
  assert.deepEqual(parse("#/channels/%E0%A4%A", HOME), HOME, "nothing throws on a bad escape");
});

test("the query: a patch removes a key on undefined, null or the empty string, sets the rest as text, keeps what it does not name, and reads back as one string", () => {
  assert.equal(queryOf(undefined), "");
  assert.equal(queryOf({}), "");
  assert.equal(queryOf({ aux: "thread", n: 3, gone: null, none: undefined, blank: "" }), "?aux=thread&n=3");
  assert.equal(applySearchPatch("?aux=thread&tab=files", { tab: null }), "?aux=thread");
  assert.equal(applySearchPatch("aux=thread", { tab: "git" }), "?aux=thread&tab=git", "with or without its question mark");
  assert.equal(applySearchPatch("?aux=thread", { aux: "" }), "", "nothing left: no question mark");
  assert.equal(applySearchPatch("", null), "");
  assert.equal(href({ name: "goal", id: "01G" }, { tab: "workflow" }), "#/goals/01G?tab=workflow");
  assert.deepEqual(splitHash("#/channels/abc?aux=thread"), { path: "/channels/abc", query: "aux=thread" });
  assert.deepEqual(splitHash(""), { path: "/", query: "" });
  assert.deepEqual(parse("#/channels/abc?aux=thread"), { name: "channel", id: "abc" }, "the query is not the route's");
});

test("a detail screen lights its section; an index lights itself", () => {
  assert.equal(section({ name: "goal", id: "x" }), "goals");
  assert.equal(section({ name: "workflow", id: "x" }), "workflows");
  assert.equal(section({ name: "run", id: "x" }), "workflows", "a run of the workspace is its workflow's");
  assert.equal(section({ name: "workbench", scope: "goal", id: "x" }), "projects");
  assert.equal(section({ name: "hosted_channel", host: "h", id: "x" }), "channels");
  assert.equal(section({ name: "hosted_dm", host: "h", id: "x" }), "messages");
  assert.equal(section({ name: "agent", id: "x" }), "agents");
  assert.equal(section({ name: "conversation", id: "x" }), "conversation", "a conversation is its own place");
  for (const name of ["pulse", "inbox", "goals", "workflows", "projects", "channels", "messages", "agents", "teams", "settings"]) assert.equal(section({ name }), name);
});

test("every route's section has an index, and the index is a screen of its own", () => {
  for (const route of EVERY_ROUTE) {
    const index = indexRouteOf(route);
    assert.equal(index.name, route.name === "conversation" ? "inbox" : section(route), "a conversation's door has no list: the Inbox");
    assert.deepEqual(parse(href(index), HOME), index, `${href(index)} is a place`);
    assert.deepEqual(indexRouteOf(index), index, "an index is its own");
  }
  assert.deepEqual(indexRouteOf({ name: "goal", id: "01G" }), { name: "goals" });
  assert.deepEqual(indexRouteOf({ name: "run", id: "01R" }), { name: "workflows" });
  assert.deepEqual(indexRouteOf({ name: "hosted_dm", host: "h", id: "d" }), { name: "messages" });
});

/**
 * Where a browser tab is at home, by name. Run with
 * `node --test desktop/src/shell/browserPlacesModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { ORIGINS, browserPlaces, footerBrowserWords, groupTabs, originOf, placeName, whereWords } from "./browserPlacesModel.mjs";

const ws = {
  goals: [
    { id: "g1", title: "Ship the storefront", statement: "x" },
    { id: "g2", title: "  ", statement: "Fix the login page\nand the rest" },
    { id: "g3", title: null, statement: "" },
  ],
  projects: [{ project: { id: "p1", slug: "bisa", name: "Bisa" } }],
  workstreams: [{ workstream: { id: "w1", project: "p1", name: null, kind: { kind: "worktree", branch: "feat/a" } }, project_name: "Bisa" }],
  channels: [{ channel: { id: "c1", name: "general" } }],
  dms: [{ channel: { id: "d1", name: "Ada, Bob" } }],
  inbox: [
    { key: "k1", kind: "conversation", title: "About the pricing page" },
    { key: "g1", kind: "goal", title: "Ship the storefront" },
  ],
};
const workflows = [{ workflow: { id: "f1", name: "Release" } }];
const places = browserPlaces(ws, workflows);
const at = (scope, id) => ({ home: { scope, id } });

test("every home has an origin, in the order a list groups them, and the workspace is the origin of a tab at home nowhere", () => {
  assert.deepEqual(
    ORIGINS.map((o) => o.kind),
    ["ide", "goal", "workflow", "channel", "dm", "conversation", "workspace"],
  );
  assert.equal(originOf({ scope: "workstream", id: "w1" }).label, "Project IDE");
  assert.equal(originOf({ scope: "work_item", id: "i1" }).label, "Project IDE", "a work item's tab is the IDE's");
  assert.equal(originOf({ scope: "goal", id: "g1" }).label, "Goal");
  assert.equal(originOf({ scope: "workflow", id: "f1" }).label, "Workflow");
  assert.equal(originOf({ scope: "channel", id: "c1" }).label, "Channel");
  assert.equal(originOf({ scope: "dm", id: "d1" }).label, "Message");
  assert.equal(originOf({ scope: "conversation", id: "k1" }).label, "Conversation");
  assert.equal(originOf(null).label, "Workspace");
  assert.equal(originOf({ scope: "nowhere", id: "x" }).label, "Workspace", "a scope nobody knows reads as the workspace");
});

test("the index names a place from the workspace's own rows: a goal by its title else its statement's first line, a workstream by project and branch, a channel, a message, a conversation, a workflow", () => {
  assert.equal(placeName({ scope: "goal", id: "g1" }, places), "Ship the storefront");
  assert.equal(placeName({ scope: "goal", id: "g2" }, places), "Fix the login page", "a blank title: the statement's first line");
  assert.equal(placeName({ scope: "goal", id: "g3" }, places), "Goal g3", "nothing said: the goal by its tail");
  assert.equal(placeName({ scope: "workstream", id: "w1" }, places), "Bisa › feat/a");
  assert.equal(placeName({ scope: "channel", id: "c1" }, places), "general");
  assert.equal(placeName({ scope: "dm", id: "d1" }, places), "Ada, Bob");
  assert.equal(placeName({ scope: "conversation", id: "k1" }, places), "About the pricing page");
  assert.equal(placeName({ scope: "workflow", id: "f1" }, places), "Release");
  assert.equal(placeName({ scope: "goal", id: "g9" }, places), null, "a goal the rows lack");
  assert.equal(placeName({ scope: "work_item", id: "i1" }, places), null, "a work item has no row to name it");
  assert.equal(placeName(null, places), null);
  assert.equal(placeName({ scope: "channel", id: "c1" }, browserPlaces({}, [])), null, "an empty index names nothing");
});

test("the words every list wears: the origin, then the name — or the kind and the id's tail while nothing names it", () => {
  assert.equal(whereWords(at("goal", "g1"), places), "Goal · Ship the storefront");
  assert.equal(whereWords(at("workstream", "w1"), places), "Project IDE · Bisa › feat/a");
  assert.equal(whereWords(at("channel", "c1"), places), "Channel · #general");
  assert.equal(whereWords(at("dm", "d1"), places), "Message · Ada, Bob");
  assert.equal(whereWords(at("conversation", "k1"), places), "Conversation · About the pricing page");
  assert.equal(whereWords(at("workflow", "f1"), places), "Workflow · Release");
  assert.equal(whereWords({ home: null }, places), "Workspace");
  assert.equal(whereWords(null, places), "Workspace");
  assert.equal(whereWords(at("goal", "01JABCDEF"), places), "Goal · goal ·ABCDEF", "not named yet: the kind and the tail, never a bare id");
  assert.equal(whereWords(at("dm", "npub1xyz"), places), "Message · message ·ub1xyz");
  assert.equal(whereWords(at("work_item", "01JITEM12"), places), "Project IDE · work item ·ITEM12");
  assert.equal(whereWords(at("workstream", "w9"), places), "Project IDE · workstream ·w9");
});

test("tabs group by origin in the list's order, empty origins left out, each group in the order opened", () => {
  const tabs = [at("channel", "c1"), at("goal", "g1"), { home: null }, at("workstream", "w1"), at("goal", "g2")];
  const groups = groupTabs(tabs);
  assert.deepEqual(
    groups.map((g) => [g.label, g.tabs.length]),
    [
      ["Project IDE", 1],
      ["Goal", 2],
      ["Channel", 1],
      ["Workspace", 1],
    ],
  );
  assert.deepEqual(groups[1].tabs.map((t) => t.home.id), ["g1", "g2"]);
  assert.deepEqual(groupTabs([]), []);
});

test("the footer's button reads how many tabs, how many out of sight, how many an agent works in, and what is showing", () => {
  assert.equal(footerBrowserWords({ count: 0, headless: 0, busy: 0, shown: null }), "No browser tab is open — New tab here, or ⌘⇧L");
  assert.equal(footerBrowserWords({ count: 1, headless: 0, busy: 0, shown: null }), "1 browser tab");
  assert.equal(footerBrowserWords({ count: 3, headless: 1, busy: 1, shown: "Pricing" }), "3 browser tabs — 1 out of sight, an agent browsing in 1; showing Pricing");
  assert.equal(footerBrowserWords({ count: 2, headless: 0, busy: 0, shown: "Docs" }), "2 browser tabs; showing Docs");
  // A number that surprises says whose it is: the agents' share leads the asides.
  assert.equal(footerBrowserWords({ count: 3, headless: 1, busy: 0, agents: 2, shown: null }), "3 browser tabs — 2 opened by agents, 1 out of sight");
  assert.equal(footerBrowserWords({ count: 1, headless: 0, busy: 0, agents: 1, shown: "Pricing" }), "1 browser tab — 1 opened by agents; showing Pricing");
  assert.equal(footerBrowserWords({ count: 2, headless: 0, busy: 0, agents: 0, shown: null }), "2 browser tabs", "none of them an agent's: nothing said");
});

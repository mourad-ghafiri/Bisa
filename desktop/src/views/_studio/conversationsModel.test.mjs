/**
 * Conversations: the words a row wears, the order a list keeps, what a filter
 * admits and where a row opens. The origin
 * kinds are read from the Rust enum, so a kind that moves in
 * `bisa-core/src/conversation.rs` fails here rather than in a picker.
 * Run with `node --test desktop/src/views/_studio/conversationsModel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import test from "node:test";
import assert from "node:assert/strict";

import {
  ORIGIN_ICON,
  ORIGIN_KINDS,
  UNTITLED,
  agentsWords,
  movedAt,
  originIcon,
  originTakesId,
  originWords,
  projectConversations,
  routeOf,
  rowWords,
  runsHere,
  sortConversations,
  titleOf,
} from "./conversationsModel.mjs";

const row = (id, origin, extra = {}) => ({
  id,
  origin,
  title: null,
  first_line: null,
  author: "me",
  created_at: 10,
  last_message_at: null,
  message_count: 0,
  archived: false,
  agents: [],
  ...extra,
});

test("the origin kinds are the Rust enum's, in its order, and only the node and the workspace stand alone", () => {
  const source = readFileSync(new URL("../../../../crates/bisa-core/src/conversation.rs", import.meta.url), "utf8");
  const block = source.slice(source.indexOf("pub const KINDS"), source.indexOf("];", source.indexOf("pub const KINDS")));
  const kinds = [...block.matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
  assert.deepEqual([...ORIGIN_KINDS], kinds);
  for (const kind of ORIGIN_KINDS) assert.equal(originTakesId(kind), kind !== "node" && kind !== "workspace", kind);
  assert.ok(Object.isFrozen(ORIGIN_KINDS));
});

test("a row is called by its title, else its first line, else the untitled word — and a long line is cut", () => {
  assert.equal(titleOf(row("c", { kind: "node" }, { title: " Ship it " })), "Ship it");
  assert.equal(titleOf(row("c", { kind: "node" }, { first_line: "which palette do we ship?" })), "which palette do we ship?");
  assert.equal(titleOf(row("c", { kind: "node" }, { title: "  ", first_line: "  words " })), "words", "a blank title is no title");
  assert.equal(titleOf(row("c", { kind: "node" })), UNTITLED);
  assert.equal(titleOf(null), UNTITLED);
  const long = "x".repeat(100);
  assert.equal(titleOf(row("c", { kind: "node" }, { first_line: long })).length, 80);
});

test("an origin is said with the names a caller knows, and by the tail of its id otherwise", () => {
  const names = {
    goal: (id) => (id === "G1" ? "Dark mode" : null),
    workflow: () => "Release",
    project: (id) => (id === "P1" ? "web-app" : null),
    workstream: (id) => (id === "W1" ? "main" : null),
  };
  assert.equal(originWords({ kind: "node" }), "this node");
  assert.equal(originWords({ kind: "workspace" }), "the workspace");
  assert.equal(originWords({ kind: "goal", id: "G1" }, names), "goal Dark mode");
  assert.equal(originWords({ kind: "goal", id: "01JXXXXXXXXXXXXXXXXXXXGONE" }, names), "goal XXGONE", "a goal the names do not hold is its id's tail");
  assert.equal(originWords({ kind: "workflow", id: "F1" }, names), "workflow Release");
  assert.equal(originWords({ kind: "project", id: "P1" }, names), "project web-app");
  assert.equal(originWords({ kind: "workstream", id: "W1", project: "P1" }, names), "workstream main of web-app");
  assert.equal(originWords({ kind: "workstream", id: "W1" }, names), "workstream main");
  assert.equal(originWords(null), "");
  assert.equal(originIcon("goal"), "goal");
  assert.equal(originIcon("node"), "settings");
  assert.equal(originIcon("workspace"), "dm");
});

test("a list keeps the most recently moved first, archived ones last, and a quiet one ranks by its birth", () => {
  const rows = [
    row("a", { kind: "node" }, { created_at: 1, last_message_at: 50 }),
    row("b", { kind: "node" }, { created_at: 60 }),
    row("c", { kind: "node" }, { created_at: 2, last_message_at: 90, archived: true }),
    row("d", { kind: "node" }, { created_at: 3, last_message_at: 50 }),
  ];
  assert.deepEqual(
    sortConversations(rows).map((r) => r.id),
    ["b", "d", "a", "c"],
    "b was born after a and d last moved; d over a by id on a tie; c is archived",
  );
  assert.equal(movedAt(rows[1]), 60);
  assert.equal(movedAt(rows[0]), 50);
  assert.deepEqual(sortConversations([]), []);
  assert.deepEqual(sortConversations(null), []);
});

test("the IDE's panel lists every conversation of the project — its own and every checkout's — live ones, newest first", () => {
  const rows = [
    row("mine", { kind: "workstream", id: "W1", project: "P1" }, { project: "P1", created_at: 5 }),
    row("project", { kind: "project", id: "P1" }, { project: "P1", created_at: 6, last_message_at: 40 }),
    row("sibling", { kind: "workstream", id: "W2", project: "P1" }, { project: "P1", created_at: 7 }),
    row("gone", { kind: "workstream", id: "W1", project: "P1" }, { project: "P1", created_at: 8, archived: true }),
    row("elsewhere", { kind: "workstream", id: "W9", project: "P2" }, { project: "P2", created_at: 9 }),
    row("goal", { kind: "goal", id: "G1" }, { created_at: 10 }),
  ];
  assert.deepEqual(projectConversations(rows, "P1").map((r) => r.id), ["project", "sibling", "mine"], "a sibling checkout's is the project's too; another project's and a goal's are not");
  assert.deepEqual(projectConversations([], "P1"), []);
  assert.deepEqual(projectConversations(rows, "P1", true).map((r) => r.id), ["gone"], "the panel's Archived switch: the archived ones, and not the live");
  assert.deepEqual(projectConversations(rows, "P3"), [], "a project with none");
});

test("a conversation's turns run here when it is about this checkout or its project — a sibling's run in the sibling", () => {
  assert.equal(runsHere({ origin: { kind: "workstream", id: "W1", project: "P1" } }, "W1", "P1"), true);
  assert.equal(runsHere({ origin: { kind: "project", id: "P1" } }, "W1", "P1"), true, "a project's turn runs in the primary, which stands for the project");
  assert.equal(runsHere({ origin: { kind: "workstream", id: "W2", project: "P1" } }, "W1", "P1"), false, "a sibling checkout's runs there");
  assert.equal(runsHere({ origin: { kind: "goal", id: "G1" } }, "W1", "P1"), false);
  assert.equal(runsHere(null, "W1", "P1"), false);
  assert.equal(runsHere({ origin: null }, "W1", "P1"), false);
});

test("a conversation opens where it is about: a checkout's in the IDE, a goal's on its tab, a workflow's in the Workflow screen's Agent mode, and one nothing owns on its own page", () => {
  assert.deepEqual(routeOf(row("c1", { kind: "workstream", id: "W1", project: "P1" })), {
    route: { name: "workbench", scope: "workstream", id: "W1" },
    search: { conversation: "c1", panel: "agents" },
  });
  assert.deepEqual(routeOf(row("c2", { kind: "project", id: "P1" })), {
    route: { name: "workbench", scope: "workstream", id: "P1" },
    search: { conversation: "c2", panel: "agents" },
  });
  assert.deepEqual(routeOf(row("c3", { kind: "goal", id: "G1" })), { route: { name: "goal", id: "G1" }, search: { tab: "conversation", conversation: "c3" } });
  assert.deepEqual(routeOf(row("c4", { kind: "workflow", id: "F1" })), { route: { name: "workflow", id: "F1" }, search: { panel: "agent", conversation: "c4" } });
  for (const origin of [{ kind: "node" }, { kind: "workspace" }]) {
    assert.deepEqual(routeOf(row("c5", origin)), { route: { name: "conversation", id: "c5" }, search: null }, "nothing owns it: the page of its own");
  }
});

test("a row's line names it, says what it is about, who is in it and how much was said", () => {
  const words = rowWords(row("c", { kind: "goal", id: "G1" }, { first_line: "hi", agents: ["general-agent", "reviewer"], message_count: 1 }), {
    names: { goal: () => "Dark mode" },
    agentName: (id) => (id === "reviewer" ? "Reviewer" : null),
  });
  assert.deepEqual(words, { title: "hi", about: "goal Dark mode", agents: "general-agent, Reviewer", count: "1 message", archived: false });
  assert.equal(agentsWords([], () => null), "");
  assert.equal(agentsWords(null, () => null), "");
});

test("every kind of the wire's ConversationOrigin has its words and its glyph — the eight, read from the generated types", () => {
  const types = readFileSync(new URL("../../types.gen.ts", import.meta.url), "utf8");
  const from = types.indexOf("export type ConversationOrigin =");
  assert.ok(from >= 0, "ConversationOrigin is declared in types.gen.ts");
  const wire = [...types.slice(from, types.indexOf(";\n/**", from)).matchAll(/kind: "([a-z_]+)";/g)].map((m) => m[1]);
  assert.deepEqual(wire, ["node", "workspace", "goal", "workflow", "project", "workstream", "drawing", "note"], "the eight kinds, in the wire's order");
  assert.deepEqual([...ORIGIN_KINDS], wire, "the model's kinds are the wire's");
  // A glyph for each, and each a name `ui/icons` has.
  assert.deepEqual(Object.keys(ORIGIN_ICON), wire, "one row a kind, none over");
  const icons = readFileSync(new URL("../../ui/icons.ts", import.meta.url), "utf8");
  for (const kind of wire) {
    assert.equal(originIcon(kind), ORIGIN_ICON[kind]);
    assert.ok(new RegExp(`^  ${ORIGIN_ICON[kind]}: `, "m").test(icons), `${kind} wears ${ORIGIN_ICON[kind]}, a glyph of ui/icons`);
  }
  assert.equal(originIcon("something_new"), "dm", "a kind a newer node names wears the conversation's own glyph");
  // Words for each: a kind that stands alone is a sentence of its own, one that names a record says the record.
  const names = { goal: () => "Dark mode", workflow: () => "Release", project: () => "web-app", workstream: () => "main", drawing: () => "Login flow", note: () => "Standup" };
  const said = Object.fromEntries(wire.map((kind) => [kind, originWords(originTakesId(kind) ? { kind, id: "01JXXXXXXXXXXXXXXXXXXXXXID" } : { kind }, names)]));
  assert.deepEqual(said, {
    node: "this node",
    workspace: "the workspace",
    goal: "goal Dark mode",
    workflow: "workflow Release",
    project: "project web-app",
    workstream: "workstream main",
    drawing: "drawing Login flow",
    note: "note Standup",
  });
  for (const kind of wire) assert.notEqual(said[kind], kind, `${kind} is said in words, not by its wire word`);
  assert.equal(originWords({ kind: "drawing", id: "01JXXXXXXXXXXXXXXXXXXXGONE" }, {}), "drawing XXGONE", "a record the names do not hold is its id's tail");
  assert.equal(originWords({ kind: "something_new" }), "something_new", "a kind a newer node names is said by its wire word, never by nothing");
});


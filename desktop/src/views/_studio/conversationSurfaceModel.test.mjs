/**
 * The conversation surface's facts: its words, which view it shows, the pick
 * as an id and its record held to the owner, the memory of picks, its query.
 * Run with `node --test --import ./src/i18n/preload.mjs src/views/_studio/conversationSurfaceModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { MAX_REMEMBERED } from "../_workbench/conversationPaneModel.mjs";
import { PAGE, PICK_KEY, belongsTo, countWords, listQuery, ownerKey, ownerPlace, parsePicks, pickedId, rememberPick, selectedOf, surfaceView, surfaceWords } from "./conversationSurfaceModel.mjs";

const GOAL = { owner: { kind: "goal", id: "G1" } };
const NOTE = { owner: { kind: "note", id: "N1" } };
const IDE = { owner: { kind: "workstream", id: "W1", project: "P1" }, project: "P1" };

test("an owner's list keeps its search under the owner's own page, or under a name where it has none", () => {
  assert.equal(ownerPlace({ kind: "goal", id: "G1" }), "/goals/G1");
  assert.equal(ownerPlace({ kind: "workflow", id: "F1" }), "/workflows/F1");
  assert.equal(ownerPlace({ kind: "note", id: "N1" }), "conversations:note:N1", "a drawer floats over any screen: no page of its own");
  assert.equal(ownerPlace({ kind: "drawing", id: "D1" }), "conversations:drawing:D1");
  assert.equal(ownerPlace({ kind: "workspace" }), "conversations:workspace");
});

test("one door label for every surface, and a hint that says what a press does now", () => {
  assert.equal(surfaceWords(GOAL, { count: 0 }).door, "Conversations");
  assert.equal(surfaceWords(IDE, { count: 3 }).door, "Conversations", "the IDE's door wears the same word — one id");
  assert.equal(surfaceWords(GOAL, { count: 0 }).hint, "No conversation about this goal yet — start one");
  assert.equal(surfaceWords(GOAL, { count: 3 }).hint, "The conversations about this goal — pick one up, or start another");
  assert.equal(surfaceWords(IDE, { count: 3 }).hint, "The conversations of this project, every checkout's — open one to continue it, or start another", "a project's list says whose");
  assert.equal(surfaceWords(GOAL, { count: 3, open: true, picked: true }).hint, "Back to the conversation", "a press shows the picked thread");
  assert.equal(surfaceWords(GOAL, { count: 3, open: true, hasFallback: true }).hint, "Hide the conversations", "a press shows the owner's own surface");
  assert.equal(surfaceWords(NOTE, { count: 3, open: true }).hint, "The conversations about this note — pick one up, or start another", "the list is the only view: the door is held, the words stand");
  assert.equal(surfaceWords(IDE, { count: 0, open: true }).hint, "The conversations of this project, every checkout's — open one to continue it, or start another");
});

test("the words name what the conversations are about, for every owner a surface can have", () => {
  assert.equal(surfaceWords(GOAL, { count: 1 }).empty, "No conversation about this goal yet.");
  assert.equal(surfaceWords({ owner: { kind: "workflow", id: "F1" } }, { count: 1 }).empty, "No conversation about this workflow yet.");
  assert.equal(surfaceWords({ owner: { kind: "workstream", id: "W1" } }, { count: 1 }).empty, "No conversation about this checkout yet.");
  assert.equal(surfaceWords({ owner: { kind: "project", id: "P1" } }, { count: 1 }).empty, "No conversation about this project yet.");
  assert.equal(surfaceWords(IDE, { count: 0 }).empty, "No conversation about this project yet.", "the IDE lists the project's: its sentence is the project's");
  assert.equal(surfaceWords({ owner: { kind: "drawing", id: "D1" } }, { count: 1 }).empty, "No conversation about this drawing yet.");
  assert.equal(surfaceWords(NOTE, { count: 1 }).empty, "No conversation about this note yet.");
  assert.match(surfaceWords({ owner: { kind: "workspace" } }, { count: 0 }).hint, /about the workspace/);
  assert.match(surfaceWords(null, { count: 0 }).hint, /about the workspace/, "no source: the workspace");
  assert.equal(surfaceWords(GOAL, { count: 0 }).reading, "Reading the conversations…");
  assert.ok(!("newTitle" in surfaceWords(GOAL, { count: 0 })), "starting asks for no name");
  assert.equal(countWords(1), "1 conversation");
  assert.equal(countWords(3), "3 conversations");
});

test("the pick is an id — the address's, else the owner's remembered one — never a guess from the rows", () => {
  assert.equal(pickedId("a", "b"), "a", "the address wins");
  assert.equal(pickedId(null, "b"), "b", "a bare address comes back to the last pick");
  assert.equal(pickedId(null, null), null);
  assert.equal(pickedId(undefined, undefined), null);
  const rows = [{ id: "a" }, { id: "b" }];
  assert.equal(selectedOf(rows, "b").id, "b", "listed: the row");
  assert.equal(selectedOf(rows, "zzz"), null, "not listed: nothing here — the record is read by id");
  assert.equal(selectedOf(rows, null), null);
  assert.equal(selectedOf([], "a"), null);
  assert.equal(selectedOf(null, "a"), null);
});

test("a conversation belongs to a surface when it is about the owner, or stands in the project a project's surface lists", () => {
  const about = (kind, id, project = null) => ({ id: "c", origin: id ? { kind, id } : { kind }, project });
  assert.equal(belongsTo(about("goal", "G1"), GOAL), true);
  assert.equal(belongsTo(about("goal", "G2"), GOAL), false, "another goal's");
  assert.equal(belongsTo(about("note", "G1"), GOAL), false, "another kind's, whatever the id");
  assert.equal(belongsTo(about("workspace", null), { owner: { kind: "workspace" } }), true, "an owner with no id");
  assert.equal(belongsTo(about("node", null), { owner: { kind: "workspace" } }), false);
  assert.equal(belongsTo(about("workstream", "W2", "P1"), IDE), true, "a sibling checkout's stands in the project: listed here");
  assert.equal(belongsTo(about("project", "P1", "P1"), IDE), true, "the project's own");
  assert.equal(belongsTo(about("workstream", "W9", "P2"), IDE), false, "another project's");
  assert.equal(belongsTo({ ...about("goal", "G1"), archived: true }, GOAL), true, "archived is still the owner's — its thread says so");
  assert.equal(belongsTo(null, GOAL), false);
  assert.equal(belongsTo({ id: "c" }, GOAL), false, "no origin, no owner");
});

test("one view at a time: a pick shows, an id being read waits, an error says so, the rows show when nothing is picked, the owner's own surface where it has one, the empty state only when there is none", () => {
  const at = (over) => ({ selected: null, settling: false, error: null, listOpen: false, loading: false, count: 0, hasFallback: false, ...over });
  assert.equal(surfaceView(at({ selected: { id: "c1" } })), "thread");
  assert.equal(surfaceView(at({ selected: { id: "c1" }, listOpen: true })), "list", "asked for the list while on a thread");
  assert.equal(surfaceView(at({ settling: true, count: 3 })), "reading", "an id named and not read yet: nothing flashes, not the list");
  assert.equal(surfaceView(at({ settling: true, hasFallback: true })), "reading");
  assert.equal(surfaceView(at({ error: "the node did not answer" })), "error", "a node that is down never reads as no conversations");
  assert.equal(surfaceView(at({ error: "down", listOpen: true, count: 0 })), "error");
  assert.equal(surfaceView(at({ listOpen: true, count: 2 })), "list");
  assert.equal(surfaceView(at({ listOpen: true, count: 0, hasFallback: true })), "fallback", "a list asked for with nothing in it: the owner's own surface");
  assert.equal(surfaceView(at({ count: 3, hasFallback: true })), "fallback", "the goal's thread stands while nothing is picked");
  assert.equal(surfaceView(at({ loading: true })), "reading", "the first page is still on its way");
  assert.equal(surfaceView(at({ count: 3 })), "list", "nothing picked, conversations there: the list — a drawer opened fresh shows them, never *none yet*");
  assert.equal(surfaceView(at({ count: 0 })), "empty", "none at all: the one sentence and New conversation");
});

test("a pick is remembered per owner under one key, the newest kept, and forgotten on back", () => {
  assert.equal(PICK_KEY, "bisa.conversations.pick");
  assert.equal(ownerKey({ kind: "workflow", id: "W1" }), "workflow:W1");
  assert.equal(ownerKey({ kind: "workstream", id: "W1" }), "workstream:W1", "the IDE's root key, letter for letter");
  assert.equal(ownerKey({ kind: "workspace" }), "workspace", "an origin with no id is its kind");
  let memory = parsePicks(null);
  assert.deepEqual(memory, {});
  memory = rememberPick(memory, "workflow:W1", "c1");
  memory = rememberPick(memory, "note:N1", "c9");
  assert.deepEqual(memory, { "workflow:W1": "c1", "note:N1": "c9" }, "each owner its own, whatever its kind");
  memory = rememberPick(memory, "workflow:W1", "c2");
  assert.equal(memory["workflow:W1"], "c2", "a new pick replaces the owner's last");
  assert.deepEqual(Object.keys(memory), ["note:N1", "workflow:W1"], "and makes the owner the newest");
  assert.equal(rememberPick(memory, "workflow:W1", "c2"), memory, "the newest pick again: the same memory, nothing to write");
  const forgot = rememberPick(memory, "workflow:W1", null);
  assert.deepEqual(forgot, { "note:N1": "c9" }, "back forgets the owner's pick");
  assert.equal(rememberPick(forgot, "workflow:W1", null), forgot, "forgetting nothing writes nothing");
  let many = {};
  for (let i = 0; i <= MAX_REMEMBERED; i += 1) many = rememberPick(many, `workflow:W${i}`, `c${i}`);
  assert.equal(Object.keys(many).length, MAX_REMEMBERED);
  assert.ok(!("workflow:W0" in many), "the oldest owner was dropped");
  assert.deepEqual(parsePicks(JSON.parse(JSON.stringify(memory))), memory);
  assert.deepEqual(parsePicks("c1"), {});
  assert.deepEqual(parsePicks(["c1"]), {});
  assert.deepEqual(parsePicks({ "workflow:W1": 3, "workflow:W2": "" }), {});
});

test("the query is the owner's own — or the project's, every checkout's — the trimmed words, live or archived, one page", () => {
  assert.deepEqual(listQuery(GOAL), { origin: "goal", id: "G1", archived: false, limit: PAGE });
  assert.deepEqual(listQuery(GOAL, { q: "  router ", archived: true }), { origin: "goal", id: "G1", q: "router", archived: true, limit: PAGE });
  assert.deepEqual(listQuery({ owner: { kind: "workspace" } }, { q: "   " }), { origin: "workspace", archived: false, limit: PAGE }, "blank words are no words; an origin with no id sends none");
  assert.deepEqual(listQuery(IDE, { q: "header" }), { project: "P1", q: "header", archived: false, limit: PAGE }, "the IDE's one read is by project, and its search is the node's too");
  assert.ok(!("origin" in listQuery(IDE)), "never both");
  assert.ok(PAGE >= 50 && PAGE <= 500, String(PAGE));
});

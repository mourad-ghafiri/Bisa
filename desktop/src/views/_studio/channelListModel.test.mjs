/**
 * The Channels and Direct-messages lists' rules, tested where they live: one
 * order per list, what a row says of its last words, how a search finds a
 * room, and how a frame off the bus keeps the preview live. No DOM.
 *
 * Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { activityOf, dmHead, dmLabel, dmOthers, filterChannels, filterDms, latestWords, participantsOf, sortChannels, sortDms, withLatest } from "./channelListModel.mjs";
import { NO_TAG_FILTER } from "../../ui/tagSearchModel.mjs";

const room = (id, created_at, latest = null, extra = {}) => ({
  channel: { id, name: id, kind: "standing", audience: { audience: "workspace" }, created_at, tags: [], topic: null, ...extra },
  unread_count: 0,
  latest,
});
const dm = (id, created_at, principals, latest = null) => ({
  channel: { id, name: "a1b2c3d4 · e5f6a7b8", kind: "direct", audience: { audience: "restricted", principals }, created_at, tags: [] },
  unread_count: 0,
  latest,
});
const said = (author, snippet, at) => ({ author, snippet, at });
const NAMES = { me: "Me", ada: "Ada", scout: "Scout" };
const nameOf = (p) => NAMES[p] ?? p.slice(0, 6);
const ids = (rows) => rows.map((r) => r.channel.id);

test("channels stand still: general first, then in creation order — the last words never reshuffle them", () => {
  const rows = [room("design", 30, said("ada", "shapes", 900)), room("general", 10), room("ops", 20, said("scout", "paging", 50))];
  assert.deepEqual(ids(sortChannels(rows)), ["general", "ops", "design"]);
  assert.deepEqual(ids(sortChannels([room("b", 5), room("a", 5)])), ["a", "b"], "born in one second: by id, stable");
  assert.deepEqual(ids(rows), ["design", "general", "ops"], "the list handed in is left as it was");
  assert.deepEqual(sortChannels(null), []);
});

test("direct channels are a recency list: the one that moved last first, a room nobody wrote in by its birth, newest first", () => {
  const rows = [dm("d1", 10, ["me", "ada"], said("ada", "hi", 100)), dm("d2", 20, ["me", "scout"]), dm("d3", 5, ["me", "ada", "scout"], said("me", "later", 300)), dm("d4", 25, ["me", "scout"])];
  assert.deepEqual(ids(sortDms(rows)), ["d3", "d1", "d4", "d2"], "words first, newest words on top; the silent rooms after, the newest born first");
  assert.equal(activityOf(rows[1]), 20, "no words: its birth");
  assert.equal(activityOf(rows[0]), 100);
  assert.deepEqual(ids(rows), ["d1", "d2", "d3", "d4"], "the list handed in is left as it was");
});

test("a channel is found by its name, its topic or a tag — case apart — and the tag facets narrow within the words", () => {
  const rows = [room("design", 1, null, { topic: "how it looks", tags: ["ux"] }), room("ops", 2, null, { topic: "paging and alerts", tags: ["infra"] }), room("general", 0)];
  assert.deepEqual(ids(filterChannels(rows, "", NO_TAG_FILTER)), ["design", "ops", "general"], "nothing typed: everything, in the order handed in");
  assert.deepEqual(ids(filterChannels(rows, "LOOKS", NO_TAG_FILTER)), ["design"], "the topic, whatever the case");
  assert.deepEqual(ids(filterChannels(rows, "infra", NO_TAG_FILTER)), ["ops"], "a tag's word");
  assert.deepEqual(ids(filterChannels(rows, "gen", NO_TAG_FILTER)), ["general"], "the name");
  assert.deepEqual(ids(filterChannels(rows, "", { selected: ["ux"], match: "any" })), ["design"], "the facet alone");
  assert.deepEqual(ids(filterChannels(rows, "paging", { selected: ["ux"], match: "any" })), [], "the words narrow within the facet, never widen");
  assert.deepEqual(filterChannels(null, "x", NO_TAG_FILTER), []);
});

test("a direct channel is known by the others in it: their names as its label, the first of them as its face, Just you when alone", () => {
  const two = dm("d1", 1, ["me", "ada", "scout"]);
  assert.deepEqual(dmOthers(two.channel, "me"), ["ada", "scout"]);
  assert.equal(dmLabel(two.channel, "me", nameOf), "Ada, Scout");
  assert.equal(dmHead(two.channel, "me"), "ada");
  const alone = dm("d2", 1, ["me"]);
  assert.equal(dmLabel(alone.channel, "me", nameOf), "Just you");
  assert.equal(dmHead(alone.channel, "me"), "d2", "nobody else: the room itself");
  assert.deepEqual(participantsOf(room("general", 0).channel), [], "a workspace-wide room lists nobody");
  assert.deepEqual(participantsOf(null), []);
  assert.notEqual(dmLabel(two.channel, "me", nameOf), two.channel.name, "never the stored name, which is hex");
});

test("a direct channel is found by a participant's name", () => {
  const rows = [dm("d1", 1, ["me", "ada"]), dm("d2", 2, ["me", "scout"]), dm("d3", 3, ["me", "ada", "scout"])];
  assert.deepEqual(ids(filterDms(rows, "ada", "me", nameOf)), ["d1", "d3"]);
  assert.deepEqual(ids(filterDms(rows, "SCOUT", "me", nameOf)), ["d2", "d3"], "case apart");
  assert.deepEqual(ids(filterDms(rows, "", "me", nameOf)), ["d1", "d2", "d3"], "nothing typed: everything");
  assert.deepEqual(ids(filterDms(rows, "me", "me", nameOf)), [], "your own name finds nothing: you are in every one");
  assert.deepEqual(ids(filterDms(rows, "nobody", "me", nameOf)), []);
});

test("the line under a row is who said the last words, and the words — or nothing, for the row to say in its own way", () => {
  assert.equal(latestWords(said("ada", "shapes are in", 100), nameOf), "Ada: shapes are in");
  assert.equal(latestWords(null, nameOf), null);
  assert.equal(latestWords(undefined, nameOf), null);
});

test("a frame off the bus puts the node's latest on the row it names, as it came, and leaves the list the same array otherwise", () => {
  const rows = [dm("d1", 1, ["me", "ada"], said("ada", "hi", 100)), dm("d2", 2, ["me", "scout"])];
  const next = withLatest(rows, { scope: "d2", kind: 3407, author: "scout", snippet: "paging you", unread_count: 1, latest: said("scout", "paging you", 500) });
  assert.notEqual(next, rows, "a new array: the row changed");
  assert.deepEqual(next[1].latest, { author: "scout", snippet: "paging you", at: 500 }, "the node's words and the node's time — no clock of ours");
  assert.equal(next[0], rows[0], "the other row keeps its identity: nothing repaints for nothing");
  assert.deepEqual(ids(sortDms(next)), ["d2", "d1"], "and the words lift the room to the top");
  assert.equal(withLatest(rows, { scope: "elsewhere", kind: 3407, latest: said("ada", "x", 500) }), rows, "a room this list does not hold: the same array");
  assert.equal(withLatest(rows, { scope: "d1", kind: 33405, snapshot: true }), rows, "a snapshot names a definition that moved, and no words");
  assert.equal(withLatest(rows, { scope: "d1", kind: 33405, snapshot: true, latest: null }), rows, "a snapshot changes nothing, whatever it carries");
  assert.equal(withLatest(rows, { scope: "d1", kind: 3407, author: "ada", snippet: "x" }), rows, "a frame with no `latest` key says nothing of the room's last words");
  assert.equal(withLatest(rows, null), rows);
  assert.equal(withLatest(rows, undefined), rows);
});

test("the row mirrors no rule: what a retraction, a reaction or a membership event leaves standing is the frame's latest, null included", () => {
  const rows = [dm("d1", 1, ["me", "ada"], said("ada", "second", 200)), dm("d2", 2, ["me", "scout"], said("scout", "only", 150))];
  // A retraction of the last post gives the place back to the one before it: the frame says which.
  const back = withLatest(rows, { scope: "d1", kind: 3408, author: "ada", snippet: "", latest: said("me", "first", 100) });
  assert.deepEqual(back[0].latest, { author: "me", snippet: "first", at: 100 }, "whatever the frame's own kind and snippet say");
  assert.deepEqual(ids(sortDms(back)), ["d2", "d1"], "and the room falls back to where its standing words put it");
  // The only post withdrawn: nothing stands, and the row says so.
  const empty = withLatest(rows, { scope: "d2", kind: 3408, author: "scout", snippet: "", latest: null });
  assert.equal(empty[1].latest, null);
  assert.equal(latestWords(empty[1].latest, nameOf), null);
  assert.equal(activityOf(empty[1]), 2, "a room where nothing stands is placed by its birth");
  // A reaction, a membership event: the frame's latest is what the row says already — the same array.
  assert.equal(withLatest(rows, { scope: "d1", kind: 7, author: "scout", snippet: "+", latest: said("ada", "second", 200) }), rows, "nothing repaints, no row is rebuilt");
  assert.equal(withLatest(empty, { scope: "d2", kind: 3407, author: "scout", snippet: "", latest: null }), empty, "nothing stood and nothing stands");
  // The model reads no kind and no snippet of the frame, and the shell reads nothing again for a frame.
  const model = readFileSync(new URL("./channelListModel.mjs", import.meta.url), "utf8");
  assert.ok(!/frame\??\.(kind|snippet|author)\b/.test(model), "the rule is the node's: neither the event's kind nor its words decide");
  const shell = readFileSync(new URL("../../shell/useWorkspaceData.ts", import.meta.url), "utf8");
  const conversation = shell.slice(shell.indexOf('if (frame.stream === "conversation")'), shell.indexOf('if (frame.stream === "inbox")'));
  assert.equal(conversation.split("withLatest(prev, f)").length - 1, 2, "both lists take the frame's latest");
  assert.equal(conversation.split("schedule()").length - 1, 1, "the lists are read again for a snapshot, and for nothing else");
  assert.ok(conversation.indexOf("schedule()") < conversation.indexOf("if (f.scope)"), "— the one read is the snapshot's");
});

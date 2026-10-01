/**
 * The Channels and Direct-messages lists as a person lives in them, stepped
 * through the models the sidebar and the two pages read — the rooms arrive
 * ordered, words land and lift a conversation, a search finds a person, the
 * rail's badge counts what is unread — and a source guard that the three
 * surfaces read one order and one preview. No DOM.
 *
 * Run with `node --test desktop/src/scenarios/channelsAndMessages.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { dmHead, dmLabel, filterChannels, filterDms, latestWords, sortChannels, sortDms, withLatest } from "../views/_studio/channelListModel.mjs";
import { NO_TURNS, applyStreamed, landedOf, retireLanded, settleTurn, turnsOf } from "../views/_studio/liveTurnModel.mjs";
import { nudgeRead } from "../views/_studio/threadCacheModel.mjs";
import { NO_TAG_FILTER } from "../ui/tagSearchModel.mjs";
import { railDoors } from "../shell/sidebarModel.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const between = (text, from, to) => text.slice(text.indexOf(from), text.indexOf(to));

const room = (id, created_at, latest = null, extra = {}) => ({ channel: { id, name: id, kind: "standing", audience: { audience: "workspace" }, created_at, tags: [], ...extra }, unread_count: 0, latest });
const dm = (id, created_at, principals, latest = null) => ({ channel: { id, name: "a1b2c3d4 · e5f6a7b8", kind: "direct", audience: { audience: "restricted", principals }, created_at, tags: [] }, unread_count: 0, latest });
const NAMES = { me: "Me", ada: "Ada", scout: "Scout" };
const nameOf = (p) => NAMES[p] ?? p.slice(0, 6);
/** The kind of event that is a message — `bisa_core::kind::KIND_MESSAGE`, the one a frame names a row for (`threadCacheModel.test.mjs` holds the number to the core). */
const KIND_MESSAGE = 3407;

test("a morning in the lists: the rooms arrive in their order, words land and lift a conversation, a search finds a person, the rail counts what is unread", () => {
  // The lists arrive. Channels: general first, then as they were made; DMs: the one that moved last first.
  let channels = [room("design", 30, { author: "ada", snippet: "shapes are in", at: 900 }, { topic: "how it looks", tags: ["ux"] }), room("general", 10), room("ops", 20)];
  let dms = [dm("d-ada", 10, ["me", "ada"], { author: "ada", snippet: "got a minute?", at: 100 }), dm("d-scout", 20, ["me", "scout"])];
  assert.deepEqual(sortChannels(channels).map((r) => r.channel.id), ["general", "ops", "design"]);
  assert.deepEqual(sortDms(dms).map((r) => r.channel.id), ["d-ada", "d-scout"]);
  // What each row says: the room's last words, a DM's other person as its name and face.
  assert.equal(latestWords(channels[0].latest, nameOf), "Ada: shapes are in");
  assert.equal(latestWords(channels[1].latest, nameOf), null, "nothing said in general yet: the row says so its own way");
  assert.equal(dmLabel(dms[1].channel, "me", nameOf), "Scout");
  assert.equal(dmHead(dms[1].channel, "me"), "scout");

  // Scout writes: the frame carries what now stands in the room, the row takes it, and the conversation lifts to the top; the channels do not move.
  const frame = { scope: "d-scout", kind: KIND_MESSAGE, author: "scout", snippet: "paging you", unread_count: 1, latest: { author: "scout", snippet: "paging you", at: 1000 } };
  dms = withLatest(dms, frame);
  channels = withLatest(channels, frame);
  assert.deepEqual(sortDms(dms).map((r) => r.channel.id), ["d-scout", "d-ada"]);
  assert.equal(latestWords(sortDms(dms)[0].latest, nameOf), "Scout: paging you");
  assert.deepEqual(sortChannels(channels).map((r) => r.channel.id), ["general", "ops", "design"], "a channel's place never follows a message");

  // Ada joins a room, then reacts: neither is anybody's last words, and the node's frames say so — the latest they carry is the one that stood.
  const before = dms;
  const stood = { author: "ada", snippet: "got a minute?", at: 100 };
  for (const quiet of [{ scope: "d-ada", kind: KIND_MESSAGE, author: "ada", snippet: "", latest: stood }, { scope: "d-ada", kind: 7, author: "ada", snippet: "+", event_id: "r1", latest: stood }]) dms = withLatest(dms, quiet);
  assert.equal(dms, before, "the same array: nothing repaints, no conversation lifts for words nobody said");
  // Scout withdraws the page: the frame says what stands now — nothing — and the row says it with no second read of the lists.
  dms = withLatest(dms, { scope: "d-scout", kind: 3408, author: "scout", snippet: "", latest: null });
  assert.equal(latestWords(dms.find((r) => r.channel.id === "d-scout").latest, nameOf), null);
  assert.deepEqual(sortDms(dms).map((r) => r.channel.id), ["d-ada", "d-scout"], "the room falls back to its place by what stands");

  // A search: a person's name finds their conversation; a topic's word finds its room.
  assert.deepEqual(filterDms(dms, "sco", "me", nameOf).map((r) => r.channel.id), ["d-scout"]);
  assert.deepEqual(filterChannels(channels, "looks", NO_TAG_FILTER).map((r) => r.channel.id), ["design"]);

  // The rail's doors count what is unread from the live map, whatever the rows' previews say.
  const doors = railDoors({ inbox: [], channels, dms, hosted: [], unread: { "d-scout": 1, design: 2 }, working: {} }, []);
  const badge = (key) => doors.find((d) => d.key === key)?.badge?.count ?? 0;
  assert.equal(badge("channels"), 2);
  assert.equal(badge("messages"), 1);
});

test("the sidebar, the Channels page and the Messages page read one order and one preview, and a direct channel is never drawn by its stored name", () => {
  const sidebar = src("../shell/Sidebar.tsx");
  assert.ok(sidebar.includes("sortChannels(ws.channels)") && sidebar.includes("sortDms(ws.dms)"), "the sidebar's sections sort through the model");
  const channels = src("../views/Channels.tsx");
  assert.ok(channels.includes("sortChannels(filterChannels(") && channels.includes("latestWords("), "the Channels page: the model's order, the model's preview");
  assert.ok(channels.includes("ws.unread[channel.id]"), "the unread count is the live map's, never the load's snapshot");
  const messages = src("../views/Messages.tsx");
  assert.ok(messages.includes("sortDms(filterDms(") && messages.includes("latestWords("), "the Messages page likewise");
  const index = between(messages, "function DmIndex(", "export default function Messages");
  assert.ok(!index.includes("channel.name"), "a direct channel's stored name is hex prefixes: the row says who is in it");
  assert.ok(index.includes("dmLabel(") && index.includes("dmHead("), "through the model");
  assert.ok(!messages.includes("`Message${"), "the dialog's button is a message of the catalog, never a template literal");
  const shell = src("../shell/useWorkspaceData.ts");
  assert.equal(shell.split("withLatest(prev, f)").length - 1, 2, "a conversation frame puts its latest on both lists' rows");
});

test("an agent's long reply lands as several messages: each frame reads its own row, the live row waits for the last, and a turn that posted nothing waits for none", () => {
  // The turn streams, then the reply is written as three messages in a row.
  let turns = applyStreamed(NO_TURNS, { scope: "c1", agent: "general-agent", text: "Part one. Part two. Part three." });
  const frames = ["m-1", "m-2", "m-3"].map((event_id) => ({ scope: "c1", kind: KIND_MESSAGE, event_id, author: "agent", snippet: "Part" }));
  assert.deepEqual(frames.map((f) => nudgeRead(f, false)), [{ read: "one", id: "m-1" }, { read: "one", id: "m-2" }, { read: "one", id: "m-3" }], "three rows, three reads — none stands in for another");
  // `agent_replied` names the last: the live row stays, frozen, until the whole reply is drawn.
  turns = settleTurn(turns, "c1", "general-agent", landedOf({ posted: true, message: "m-3" }), true);
  assert.equal(turnsOf(turns, "c1")[0].landed, "m-3");
  assert.equal(retireLanded(turns, new Set(["m-1", "m-2"])), turns, "the first two are drawn: the row still stands for the rest");
  assert.equal(retireLanded(turns, new Set(["m-1", "m-2", "m-3"])).has("c1"), false, "the last one drawn: the row gives way");
  // A turn that acted through tools alone: nothing landed, nothing is waited for.
  let quiet = applyStreamed(NO_TURNS, { scope: "c1", agent: "reviewer", text: "", thinking: "checking" });
  quiet = settleTurn(quiet, "c1", "reviewer", landedOf({ posted: false, message: null }), true);
  assert.equal(quiet.has("c1"), false);
  // The thread's hook holds the two kinds of read apart, and the artifact pane lists one conversation's artifacts at a time.
  const hook = src("../views/_studio/useScopeMessages.ts");
  assert.ok(hook.includes("const ones = useRef(new Set<AbortController>());") && hook.includes("queue.current = queue.current"));
  const pane = src("../shell/ArtifactPane.tsx");
  assert.ok(pane.includes("useEffect(() => setGallery([]), [scope]);"), "another conversation's artifacts are never listed under this one");
});


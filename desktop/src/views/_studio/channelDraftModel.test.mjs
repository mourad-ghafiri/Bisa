import { strict as assert } from "node:assert";
import { test } from "node:test";
import { dirty, mayClose } from "../_work/agentDraftModel.mjs";
import {
  channelCreateOf,
  channelDraftOf,
  channelSaveOf,
  channelStamp,
  editsRoster,
  emptyChannelDraft,
  mayCreateChannel,
  mayDeleteChannel,
  mayEditChannel,
  reachOf,
  shownChannel,
  unresolvedRoster,
} from "./channelDraftModel.mjs";

const person = "ab".repeat(32);
const design = {
  id: "design",
  name: "Design",
  topic: "how it looks",
  kind: "standing",
  roster: { policy: "listed", agents: ["scout", "ranger"], teams: ["crew"], humans: [person] },
  tags: ["ux"],
};
const general = { id: "general", name: "general", topic: "everything", kind: "standing", roster: { policy: "everyone" }, tags: [] };
const direct = { id: "dm-01", name: "ab · cd", kind: "direct", roster: { policy: "listed", agents: [], teams: [], humans: [] } };

test("a channel's draft is its topic, the two halves the dialog edits, and its tags", () => {
  assert.deepEqual(channelDraftOf(design), { name: "Design", topic: "how it looks", agents: ["scout", "ranger"], humans: [person], tags: ["ux"] });
  assert.deepEqual(channelDraftOf(general), { name: "general", topic: "everything", agents: [], humans: [], tags: [] });
  assert.deepEqual(channelDraftOf({ name: "Bare" }), { name: "Bare", topic: "", agents: [], humans: [], tags: [] });
  assert.deepEqual(emptyChannelDraft(), { name: "", topic: "", agents: [], humans: [], tags: [] });
});

test("a save names only the fields the dialog changed; a roster nobody touched is not written", () => {
  const seeded = channelDraftOf(design);
  // The topic alone: an agent put on the roster elsewhere meanwhile stays on it.
  assert.deepEqual(channelSaveOf({ ...seeded, topic: " the look of it " }, design, seeded), { topic: "the look of it" });
  // Nothing changed sends nothing; a field typed and typed back is no change.
  assert.deepEqual(channelSaveOf({ ...seeded, topic: "how it looks " }, design, seeded), {});
  // A topic emptied is sent as "" — how the node hears "clear it"; null reads there as a field nobody named.
  assert.deepEqual(channelSaveOf({ ...seeded, topic: "  " }, design, seeded), { topic: "" });
  // Each half on its own, and the tags.
  assert.deepEqual(channelSaveOf({ ...seeded, agents: ["scout"] }, design, seeded), { agents: ["scout"] });
  assert.deepEqual(channelSaveOf({ ...seeded, humans: [] }, design, seeded), { humans: [] });
  assert.deepEqual(channelSaveOf({ ...seeded, tags: ["ux", "ui"] }, design, seeded), { tags: ["ux", "ui"] });
  // A roster re-picked in another order is a change: the order is the picker's.
  assert.deepEqual(channelSaveOf({ ...seeded, agents: ["ranger", "scout"] }, design, seeded), { agents: ["ranger", "scout"] });
});

test("general's editor offers the topic and the tags only, and a save of it never names a roster", () => {
  assert.equal(editsRoster(design), true);
  assert.equal(editsRoster(general), false);
  assert.equal(editsRoster(direct), false);
  assert.equal(editsRoster(null), false);
  const seeded = channelDraftOf(general);
  assert.deepEqual(channelSaveOf({ ...seeded, topic: "all hands", agents: ["scout"], humans: [person], tags: ["all"] }, general, seeded), { topic: "all hands", tags: ["all"] });
});

test("a standing channel is edited, any channel but general is deleted", () => {
  assert.equal(mayEditChannel(design), true);
  assert.equal(mayEditChannel(general), true);
  assert.equal(mayEditChannel(direct), false);
  assert.equal(mayEditChannel(null), false);
  assert.equal(mayDeleteChannel(design), true);
  assert.equal(mayDeleteChannel(direct), true, "a direct channel is deleted as a standing one is");
  assert.equal(mayDeleteChannel(general), false);
  assert.equal(mayDeleteChannel(undefined), false);
});

test("a new channel needs a name, and is made of what the dialog holds", () => {
  assert.equal(mayCreateChannel({ name: "  " }, false), false);
  assert.equal(mayCreateChannel({ name: "Design" }, true), false, "no second create while one is out");
  assert.equal(mayCreateChannel({ name: "Design" }, false), true);
  assert.deepEqual(channelCreateOf({ name: " Design ", topic: " how it looks ", agents: ["scout"], humans: [person], tags: ["ux"] }), {
    name: "Design",
    topic: "how it looks",
    agents: ["scout"],
    humans: [person],
    tags: ["ux"],
  });
  assert.deepEqual(channelCreateOf({ ...emptyChannelDraft(), name: "Bare" }), { name: "Bare", agents: [], humans: [], tags: [] });
});

test("a dialog with edits asks before it closes, and one that is saving stays", () => {
  const seeded = channelDraftOf(design);
  assert.equal(mayClose({ dirty: dirty(seeded, seeded), busy: false }), "close");
  assert.equal(mayClose({ dirty: dirty({ ...seeded, topic: "typed" }, seeded), busy: false }), "ask");
  assert.equal(mayClose({ dirty: dirty({ ...seeded, topic: "typed" }, seeded), busy: true }), "stay");
  const fresh = emptyChannelDraft();
  assert.equal(mayClose({ dirty: dirty({ ...fresh, name: "D" }, fresh), busy: false }), "ask");
});

test("a page draws the list's copy of its channel, and what it read when the list holds none", () => {
  const stale = { ...design, topic: "as the page read it" };
  assert.equal(shownChannel([{ channel: general }, { channel: design }], "design", stale), design);
  assert.equal(shownChannel([{ channel: general }], "design", stale), stale, "a direct channel is in no standing list");
  assert.equal(shownChannel(null, "design", null), null);
  assert.equal(shownChannel([], undefined, undefined), null);
  // The stamp moves with the record, and with nothing else.
  assert.equal(channelStamp(design), channelStamp({ ...design }));
  assert.notEqual(channelStamp(design), channelStamp({ ...design, topic: "another" }));
  assert.equal(channelStamp(null), "");
});

test("a roster name no agent answers to is listed, so it can be taken off", () => {
  const agents = [{ id: "scout" }];
  assert.deepEqual(unresolvedRoster(["scout", "ghost", "gone"], agents), ["ghost", "gone"]);
  assert.deepEqual(unresolvedRoster(["scout"], agents), []);
  assert.deepEqual(unresolvedRoster(undefined, undefined), []);
});

test("who a handle reaches is the node's answer, and unknown until it answers", () => {
  assert.equal(reachOf({ roster_pubkeys: ["a", "b", "c", "d"] }), 4);
  assert.equal(reachOf({ roster_pubkeys: [] }), 0);
  assert.equal(reachOf({}), null);
  assert.equal(reachOf(null), null);
});

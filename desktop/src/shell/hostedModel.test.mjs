import assert from "node:assert/strict";
import test from "node:test";
import {
  hostName,
  hostedMentionables,
  hostedNameOf,
  hostedPage,
  hostedPhotoOf,
  hostedScreen,
  hostedRead,
  hostedUnread,
  isMember,
  orderSections,
  sectionTitle,
  stateWords,
  unreadOf,
} from "./hostedModel.mjs";

const hosted = (over = {}) => ({
  host: { pubkey: "ab".repeat(32), name: "Acme", relays: [] },
  role: "guest",
  state: { state: "member" },
  requested_at: 1,
  ...over,
});

test("a host's name and section title, and the state under it", () => {
  assert.equal(hostName(hosted()), "Acme");
  assert.equal(hostName(hosted({ host: { pubkey: "cd".repeat(32), name: "", relays: [] } })), "cdcdcdcd…");
  assert.equal(sectionTitle(hosted()), "Hosted by Acme");
  assert.equal(stateWords(hosted()), null);
  assert.equal(stateWords(hosted({ state: { state: "requested" } })), "waiting for the host");
  assert.equal(stateWords(hosted({ state: { state: "refused", reason: "expired" } })), "refused — expired");
  assert.equal(stateWords(hosted({ state: { state: "removed", reason: null } })), "removed by the host");
  assert.equal(stateWords(hosted({ state: { state: "left" } })), "you left");
  assert.equal(isMember(hosted({ state: { state: "left" } })), false);
});

test("sections come members first then by name, with their unread summed", () => {
  const a = { host: hosted({ host: { pubkey: "1".repeat(64), name: "Zed", relays: [] } }), channels: [{ unread_count: 2 }], dms: [{ unread_count: 1 }] };
  const b = { host: hosted({ host: { pubkey: "2".repeat(64), name: "Amy", relays: [] }, state: { state: "left" } }), channels: [], dms: [] };
  const c = { host: hosted({ host: { pubkey: "3".repeat(64), name: "Bea", relays: [] } }), channels: [], dms: [] };
  assert.deepEqual(orderSections([b, a, c]).map((s) => hostName(s.host)), ["Bea", "Zed", "Amy"]);
  assert.equal(unreadOf(a), 3);
  assert.equal(unreadOf(null), 0);
});

test("a hosted message becomes the chat's row with its reactions folded out", () => {
  const page = hostedPage([
    {
      id: "m1",
      scope: "general",
      author: "a".repeat(64),
      at: 10,
      text: "hi",
      reply_to: null,
      retracted: false,
      attachments: [{ sha256: "x", name: "f", mime: "text/plain", size: 1 }],
      reactions: [{ author: "b".repeat(64), emoji: "👍" }],
    },
  ]);
  assert.equal(page.messages.length, 1);
  assert.equal(page.messages[0].content, "hi");
  assert.equal(page.messages[0].scope_id, "general");
  assert.equal(page.messages[0].attachments[0].present, false);
  assert.deepEqual(page.reactions.map((r) => [r.id, r.target, r.emoji]), [[`m1:${"b".repeat(64)}:👍`, "m1", "👍"]]);
  assert.deepEqual(hostedPage(null), { messages: [], reactions: [] });
});

test("names come from the host's directory, and mentionables leave you out", () => {
  const dir = [
    { pubkey: "a".repeat(64), role: "owner", label: "Alice" },
    { pubkey: "b".repeat(64), role: "guest", label: null },
  ];
  assert.equal(hostedNameOf(dir, "a".repeat(64)), "Alice");
  assert.equal(hostedNameOf(dir, "b".repeat(64)), "bbbbbbbb…");
  assert.equal(hostedNameOf(dir, "c".repeat(64)), "cccccccc…");
  const face = { sha256: "f".repeat(64), name: "alice-face.jpg", mime: "image/jpeg", size: 700 };
  const faced = [{ ...dir[0], photo: face }, dir[1]];
  assert.equal(hostedPhotoOf(faced, "a".repeat(64)), face, "the directory's face, by hash");
  assert.equal(hostedPhotoOf(faced, "b".repeat(64)), null, "a member without one");
  assert.equal(hostedPhotoOf(faced, "c".repeat(64)), null, "a stranger");
  assert.equal(hostedPhotoOf(null, "a".repeat(64)), null);
  const m = hostedMentionables(dir, "b".repeat(64));
  assert.deepEqual(m.map((x) => [x.id, x.name, x.kind, x.description]), [["a".repeat(64), "Alice", "human", "Owner"]], "the role in the catalog's word, never the wire's");
  assert.equal(hostedNameOf(dir, ""), "someone");
});

test("one hosted scope's unread is read from its section, and marking it read patches that entry alone", () => {
  const acme = "ab".repeat(32);
  const other = "cd".repeat(32);
  const sections = [
    { host: hosted(), channels: [{ channel: { id: "general" }, unread_count: 3 }, { channel: { id: "random" }, unread_count: 1 }], dms: [{ channel: { id: "dm1" }, unread_count: 2 }] },
    { host: hosted({ host: { pubkey: other, name: "Other", relays: [] } }), channels: [{ channel: { id: "general" }, unread_count: 5 }], dms: [] },
  ];
  assert.equal(hostedUnread(sections, acme, "general"), 3, "the entry of that host, not another host's channel of the same name");
  assert.equal(hostedUnread(sections, acme, "dm1"), 2, "a direct channel counts too");
  assert.equal(hostedUnread(sections, acme, "nowhere"), 0, "a scope the host does not list");
  assert.equal(hostedUnread(sections, "ef".repeat(32), "general"), 0, "a host this node is not a guest of");
  assert.equal(hostedUnread(null, acme, "general"), 0);

  const next = hostedRead(sections, acme, "general");
  assert.notEqual(next, sections);
  assert.equal(hostedUnread(next, acme, "general"), 0);
  assert.equal(hostedUnread(next, acme, "random"), 1, "the other channel is untouched");
  assert.equal(next[1], sections[1], "the other host's section is the same object");
  assert.equal(hostedRead(next, acme, "general"), next, "a read entry changes nothing");
  assert.equal(hostedRead(sections, acme, "nowhere"), sections, "an unknown scope changes nothing");
  assert.equal(hostedRead(sections, "ef".repeat(32), "general"), sections, "an unknown host changes nothing");
});

test("a hosted screen says what it knows: reading until the memberships answered, then the membership, then the channel", () => {
  const member = { host: hosted(), channels: [{ channel: { id: "general" }, unread_count: 0 }], dms: [] };
  const left = { host: hosted({ state: { state: "left" } }), channels: [], dms: [] };
  const general = member.channels[0].channel;
  assert.equal(hostedScreen({ ready: false, section: null, channel: null }), "reading", "the workspace is not read yet: *not a member* would be a claim nobody checked");
  assert.equal(hostedScreen({ ready: true, section: null, channel: null }), "unknown-host");
  assert.equal(hostedScreen({ ready: true, section: left, channel: null }), "not-member");
  assert.equal(hostedScreen({ ready: true, section: member, channel: null }), "unreached", "a member, and a channel the role does not reach or that is gone");
  assert.equal(hostedScreen({ ready: true, section: member, channel: general }), "thread");
  assert.equal(hostedScreen({ ready: false, section: member, channel: general }), "thread", "what is already held is drawn while a load is out");
});

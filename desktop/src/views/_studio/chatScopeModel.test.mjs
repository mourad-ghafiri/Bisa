/**
 * A conversation surface's place and the tray's words. Run with
 * `node --test desktop/src/views/_studio/chatScopeModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { attachWords, attachedWords, chatKey } from "./chatScopeModel.mjs";

test("a conversation's chips are kept under kind and id, the shape the IDE's root key has", () => {
  assert.equal(chatKey("goal", "g1"), "goal:g1");
  assert.equal(chatKey("channel", "c9"), "channel:c9");
  assert.equal(chatKey("dm", "npub1x"), "dm:npub1x");
  assert.equal(chatKey("conversation", "k2"), "conversation:k2");
});

test("the attach door names the message it fills and is held with a reason when no conversation is on screen", () => {
  assert.deepEqual(attachWords(null).enabled, false);
  assert.match(attachWords(null).hint, /Open a goal, a channel or a message/);
  for (const [kind, words] of [
    ["goal", "the goal's message"],
    ["channel", "the channel's message"],
    ["dm", "the message"],
    ["conversation", "the conversation's message"],
  ]) {
    const w = attachWords({ kind, id: "x" });
    assert.ok(w.enabled);
    assert.ok(w.hint.includes(words), `${kind}: ${w.hint}`);
    assert.ok(w.hint.includes("nothing is sent"), "attaching sends nothing");
  }
  assert.equal(attachedWords(1), "Attached — write the message beside the page.");
  assert.equal(attachedWords(3), "Attached 3 annotations — write the message beside the page.");
});

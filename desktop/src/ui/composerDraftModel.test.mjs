import { strict as assert } from "node:assert";
import { test } from "node:test";
import { afterSend, draftAfterFailure, draftWhileSending } from "./composerDraftModel.mjs";

test("a send clears only the text it sent", () => {
  assert.equal(afterSend("hello", "hello"), "", "nothing typed since: the box is empty");
  // The next line typed while the send was out stays, without the words that went.
  assert.equal(afterSend("hello\nand one more thing", "hello"), "and one more thing");
  assert.equal(afterSend("hello and", "hello"), "and");
  // The box was rewritten meanwhile: it is the person's, as it stands.
  assert.equal(afterSend("something else", "hello"), "something else");
  assert.equal(afterSend("", "hello"), "");
  assert.equal(afterSend("typed", ""), "typed", "a send of files alone clears no words");
});

test("the draft is forgotten when the send is asked, and put back if it fails", () => {
  assert.equal(draftWhileSending(), null);
  assert.equal(draftAfterFailure("hello", "hello"), "hello");
  assert.equal(draftAfterFailure("hello and more", "hello"), "hello and more", "what the box holds now is what is kept");
  assert.equal(draftAfterFailure(null, "hello"), "hello", "the room was left before the answer: the words are kept for it");
  assert.equal(draftAfterFailure("   ", "hello"), "hello");
  assert.equal(draftAfterFailure("", ""), null);
});

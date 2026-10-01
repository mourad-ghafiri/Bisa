/**
 * A streaming reply cut into settled blocks and a live tail. Run with
 * `node --test desktop/src/views/_studio/streamBlocksModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { settledBlocks } from "./streamBlocksModel.mjs";

const whole = (text) => [...settledBlocks(text).settled, settledBlocks(text).tail].join("\n");

test("a blank line outside a fence closes a block, and the tail is what is still being written", () => {
  const { settled, tail } = settledBlocks("# Title\n\nFirst para\nstill first\n\nSecond, unfini");
  assert.deepEqual(settled, ["# Title\n", "First para\nstill first\n"]);
  assert.equal(tail, "Second, unfini");
});

test("an open fence never closes on a blank line — only its closing marker settles it", () => {
  const open = "Look:\n\n```ts\nconst a = 1;\n\nconst b = 2;";
  assert.deepEqual(settledBlocks(open).settled, ["Look:\n"]);
  assert.equal(settledBlocks(open).tail, "```ts\nconst a = 1;\n\nconst b = 2;");
  const closed = `${open}\n\`\`\`\n\nAfter`;
  assert.deepEqual(settledBlocks(closed).settled, ["Look:\n", "```ts\nconst a = 1;\n\nconst b = 2;\n```\n"]);
  assert.equal(settledBlocks(closed).tail, "After");
  assert.equal(settledBlocks("~~~\nx\n\n```\nstill in the tilde fence").tail, "~~~\nx\n\n```\nstill in the tilde fence", "a backtick line does not close a tilde fence");
});

test("a list continued past a blank line, or a table, stays one block", () => {
  const list = "- one\n\n  more of one\n\n- two\n\nDone";
  assert.deepEqual(settledBlocks(list).settled, ["- one\n\n  more of one\n", "- two\n"]);
  const table = "| a | b |\n|---|---|\n\n| 1 | 2 |\n\nAfter";
  assert.deepEqual(settledBlocks(table).settled, ["| a | b |\n|---|---|\n\n| 1 | 2 |\n"]);
});

test("the settled blocks and the tail are the text again, exactly", () => {
  for (const text of ["", "one", "one\n\ntwo", "one\n\n\n\ntwo\n", "```\nopen", "a\n\n```\nb\n```\n\nc\n\n"]) {
    assert.equal(whole(text), text);
  }
  assert.deepEqual(settledBlocks("").settled, []);
  assert.equal(settledBlocks("\n\n").tail, "\n\n", "blank alone settles nothing");
  assert.deepEqual(settledBlocks("- one\n\n").settled, [], "a blank line with nothing after it yet settles nothing: the list may continue");
  assert.equal(settledBlocks("- one\n\n").tail, "- one\n\n");
});

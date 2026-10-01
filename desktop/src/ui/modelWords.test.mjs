/**
 * The model a session runs on, as words. Run with
 * `node --test desktop/src/ui/modelWords.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { modelWords, nameWithModel } from "./modelWords.mjs";

test("a provider prefix folds away for the row and stays for the tooltip; nothing is guessed", () => {
  assert.deepEqual(modelWords("anthropic/claude-opus-5"), { short: "claude-opus-5", full: "anthropic/claude-opus-5" });
  assert.deepEqual(modelWords("openai/gpt-5.5-codex"), { short: "gpt-5.5-codex", full: "openai/gpt-5.5-codex" });
  assert.deepEqual(modelWords("claude-opus-5"), { short: "claude-opus-5", full: "claude-opus-5" }, "no prefix, no change");
  assert.deepEqual(modelWords("  opus  "), { short: "opus", full: "opus" });
  assert.deepEqual(modelWords("anthropic/"), { short: "anthropic/", full: "anthropic/" }, "a trailing slash is not a prefix");
  assert.equal(modelWords(""), null);
  assert.equal(modelWords(null), null);
  assert.equal(modelWords(undefined), null);
});

test("a name reads with its model, or alone", () => {
  assert.equal(nameWithModel("general-agent", "anthropic/claude-opus-5"), "general-agent · claude-opus-5");
  assert.equal(nameWithModel("Claude Code", null), "Claude Code");
  assert.equal(nameWithModel("general-agent", "claude-opus-5-5[1m]", "high"), "general-agent · claude-opus-5-5[1m] · high");
});

test("the effort follows the model only when the row carries one, as the wire's own word", () => {
  assert.deepEqual(modelWords("claude-opus-5-5[1m]", "high"), { short: "claude-opus-5-5[1m] · high", full: "claude-opus-5-5[1m] · high" });
  assert.deepEqual(modelWords("anthropic/claude-sonnet-5-5[1m]", "xhigh"), { short: "claude-sonnet-5-5[1m] · xhigh", full: "anthropic/claude-sonnet-5-5[1m] · xhigh" }, "the prefix folds, the level is the wire's word");
  for (const none of [null, undefined, "", "  "]) {
    assert.deepEqual(modelWords("claude-opus-5-5[1m]", none), { short: "claude-opus-5-5[1m]", full: "claude-opus-5-5[1m]" }, "no effort on the row, none shown");
  }
  assert.deepEqual(modelWords(null, "max"), { short: "max", full: "max" }, "the harness's own model at a level the platform named");
  assert.equal(modelWords(null, null), null, "nothing known, nothing said");
  assert.equal(modelWords("", ""), null);
});

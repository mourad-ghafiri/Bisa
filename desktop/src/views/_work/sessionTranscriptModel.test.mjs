/**
 * The words around a session's transcript. Run with `node --test desktop/src/views/_work/sessionTranscriptModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { transcriptTabTitle, transcriptTitle, transcriptWords } from "./sessionTranscriptModel.mjs";

test("the title names the agent, then the harness and model behind it; a harness alone when no agent persona runs", () => {
  assert.equal(transcriptTitle({ agent: "dev", harness: "claude-code", model: "claude-sonnet-4" }), "dev · claude-code · claude-sonnet-4", "the harness-native id, its provider prefix folded (modelWords)");
  assert.equal(transcriptTitle({ agent: null, harness: "claude-code", model: null }), "claude-code");
  assert.equal(transcriptTitle({ agent: "dev", harness: "codex", model: null }), "dev · codex");
  assert.equal(transcriptTitle({ agent: "dev", harness: "claude-code", model: "claude-opus-5-5[1m]", effort: "high" }), "dev · claude-code · claude-opus-5-5[1m] · high", "the effort the session runs at, when the row carries one");
  assert.equal(transcriptTitle({ agent: null, harness: "claude-code", model: "claude-opus-5-5[1m]", effort: null }), "claude-code · claude-opus-5-5[1m]", "and none when it carries none");
  assert.equal(transcriptTitle(null), "Transcript", "a session the roster has forgotten still has a name for the pane");
  assert.equal(transcriptTabTitle({ agent: "dev", harness: "codex" }), "dev");
  assert.equal(transcriptTabTitle({ agent: null, harness: "codex" }), "codex");
  assert.equal(transcriptTabTitle(null), "");
});

test("the line under the title says the state and whether the tail still follows", () => {
  assert.deepEqual(transcriptWords({ state: { state: "running", tool: "Bash", args: "cargo test" } }), { words: "running Bash · cargo test — following as it writes", live: true });
  assert.deepEqual(transcriptWords({ state: "idle" }), { words: "idle — following as it writes", live: true }, "idle between turns may still write on the next one");
  assert.deepEqual(transcriptWords({ state: "done" }), { words: "done — the transcript is complete", live: false });
  assert.equal(transcriptWords({ state: { state: "failed", reason: "boom" } }).words, "failed: boom — the transcript is complete");
  assert.deepEqual(transcriptWords(null), { words: "The session is gone from the roster — what it wrote stays.", live: false });
});

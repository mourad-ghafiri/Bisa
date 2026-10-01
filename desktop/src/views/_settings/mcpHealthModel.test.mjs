/**
 * A server's health and a probe's report, in words. Run with
 * `node --test desktop/src/views/_settings/mcpHealthModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { CHECK_ALL_AT_ONCE, capabilityWords, checkedWords, eraWords, healthTone, healthWords, reportLines, stageWords } from "./mcpHealthModel.mjs";

test("a health is one of three tones and reads as what the server said, or where it stopped", () => {
  assert.equal(healthTone(undefined), "quiet");
  assert.equal(healthTone({ state: "unknown" }), "quiet");
  assert.equal(healthTone({ state: "ok" }), "ok");
  assert.equal(healthTone({ state: "failing" }), "warn");
  assert.equal(healthWords(null), "not checked yet");
  assert.equal(healthWords({ state: "ok", server: { name: "docs", version: "1.4.2" }, protocol_version: "2026-07-28", tool_count: 12 }), "docs 1.4.2 · 2026-07-28 · 12 tools");
  assert.equal(healthWords({ state: "ok", server: { name: "docs", version: "" }, tool_count: 1 }), "docs · 1 tool");
  assert.equal(healthWords({ state: "failing", stage: "spawn" }), "could not start the command or reach the URL");
  assert.equal(stageWords("initialize"), "reached the server, but the MCP handshake did not complete");
  assert.equal(stageWords("ping"), "answered the handshake, but not a ping");
  assert.equal(stageWords("tools"), "answered the handshake, but not the tool list");
  assert.equal(stageWords(undefined), "did not answer");
  assert.match(eraWords("discover"), /2026-07-28/);
  assert.match(eraWords("handshake"), /2024-11-05/);
  assert.equal(eraWords(undefined), "");
});

test("how old an answer is, in a person's words", () => {
  assert.equal(checkedWords(null), "");
  assert.equal(checkedWords(1000, 1030), "checked just now");
  assert.equal(checkedWords(1000, 1000 + 120), "checked 2 min ago");
  assert.equal(checkedWords(1000, 1000 + 7200), "checked 2 h ago");
  assert.equal(checkedWords(1000, 1000 + 172800), "checked 2 d ago");
});

test("a report's lines say who answered, what it can do and the tools — or where it stopped", () => {
  const ok = reportLines({
    ok: true,
    transport: "http",
    era: "discover",
    protocol_version: "2026-07-28",
    server: { name: "docs", version: "1.4.2" },
    capabilities: { tools: true, resources: true, prompts: false, logging: false, completions: false },
    tools: [{ name: "search", description: "Search" }],
    tool_count: 60,
    resource_count: 3,
    elapsed_ms: 120,
    stage: "done",
  });
  assert.equal(ok.ok, true);
  assert.equal(ok.headline, "docs 1.4.2 answered · 60 tools");
  assert.deepEqual(ok.detail, ["protocol 2026-07-28 · discover (2026-07-28 onwards)", "capabilities: tools, resources", "3 resources", "120 ms"]);
  assert.equal(ok.more, 59, "the count is the truth, the names are bounded");
  assert.deepEqual(capabilityWords({ tools: true, logging: true }), ["tools", "logging"]);
  const bad = reportLines({ ok: false, transport: "stdio", capabilities: {}, tools: [], tool_count: 0, elapsed_ms: 3001, stage: "initialize", error: "no answer within 3 s" });
  assert.equal(bad.ok, false);
  assert.equal(bad.headline, "reached the server, but the MCP handshake did not complete");
  assert.deepEqual(bad.detail, ["no answer within 3 s", "stopped at: initialize", "3001 ms"]);
  assert.ok(CHECK_ALL_AT_ONCE >= 2 && CHECK_ALL_AT_ONCE <= 8);
});

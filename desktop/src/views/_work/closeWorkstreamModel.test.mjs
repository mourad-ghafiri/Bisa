/**
 * What a close terminates. Run with
 * `node --test desktop/src/views/_work/closeWorkstreamModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { terminationConsent, terminationCounts, terminationWords } from "./closeWorkstreamModel.mjs";

/** The roster kinds, as the store spells them — the model excludes `terminal`, and the fixtures below use the Rust's words. */
const RUST_KINDS = [...readFileSync(new URL("../../../../crates/bisa-store/src/index.rs", import.meta.url), "utf8").match(/pub enum SessionKind \{([\s\S]*?)\n\}/)[1].matchAll(/^\s{4}([A-Z][a-z]+),/gm)].map((m) => m[1].toLowerCase());

const tab = (id, harness, status = "live", over = {}) => ({ scope: "workstream", id, harness, liveness: { status, code: null }, ...over });
const row = (workstream, kind, state) => ({ workstream, kind, state });

test("a close counts the live tabs rooted at the workstream — harnesses apart from shells — and the engine's live sessions there, once each", () => {
  const terminals = [
    tab("w1", "claude-code"),
    tab("w1", null),
    tab("w1", null),
    tab("w1", "codex", "exited"),
    tab("w2", "claude-code"),
    { ...tab("w1", null), scope: "goal" },
  ];
  const sessions = [
    row("w1", "terminal", "running"),
    row("w1", "worker", "thinking"),
    row("w1", "worker", "idle"),
    row("w1", "worker", "done"),
    row("w1", "guided", "aborted"),
    row("w2", "worker", "running"),
    row(null, "worker", "running"),
  ];
  assert.deepEqual(terminationCounts(sessions, terminals, "w1"), { harnesses: 1, shells: 2, agents: 2 });
  assert.deepEqual(terminationCounts(sessions, terminals, "w2"), { harnesses: 1, shells: 0, agents: 1 });
  assert.deepEqual(terminationCounts(null, null, "w1"), { harnesses: 0, shells: 0, agents: 0 });
});

test("the fixtures speak the store's kinds: `terminal` is one, `interactive` is not", () => {
  assert.ok(RUST_KINDS.includes("terminal") && RUST_KINDS.includes("worker"), `the store's kinds: ${RUST_KINDS.join(", ")}`);
  assert.ok(!RUST_KINDS.includes("interactive"), "the old word is gone from the Rust, so it is gone from here");
});

test("an exited tab is not terminated, a terminal row is its tab and not a second count, an idle engine session is live and a done one is not", () => {
  assert.deepEqual(terminationCounts([], [tab("w1", "codex", "exited"), tab("w1", null, "lost")], "w1"), { harnesses: 0, shells: 0, agents: 0 });
  assert.deepEqual(terminationCounts([row("w1", "terminal", "running")], [tab("w1", "codex")], "w1"), { harnesses: 1, shells: 0, agents: 0 }, "the tab is the row");
  assert.deepEqual(terminationCounts([row("w1", "worker", "idle")], [], "w1"), { harnesses: 0, shells: 0, agents: 1 }, "idle between turns is live");
  assert.deepEqual(terminationCounts([row("w1", "worker", "done"), row("w1", "worker", { state: "failed", reason: "x" })], [], "w1"), { harnesses: 0, shells: 0, agents: 0 });
  assert.deepEqual(terminationCounts([row("w1", "worker", { state: "waiting", on: { on: "auth", provider: "x" } })], [], "w1"), { harnesses: 0, shells: 0, agents: 1 }, "waiting on the person is live");
});

test("the words name each kind in its own verb — terminated, closed, aborted — singular or plural, joined with *and*; nothing says nothing", () => {
  assert.equal(terminationWords({ harnesses: 1, shells: 2, agents: 1 }), "1 harness terminated, 2 shells closed and 1 agent session aborted");
  assert.equal(terminationWords({ harnesses: 2, shells: 0, agents: 0 }), "2 harnesses terminated");
  assert.equal(terminationWords({ harnesses: 0, shells: 1, agents: 2 }), "1 shell closed and 2 agent sessions aborted");
  assert.equal(terminationWords({ harnesses: 0, shells: 0, agents: 0 }), null);
  // A word's number is the language's rule, chosen in the message: no plural is built in code.
  const model = readFileSync(new URL("./closeWorkstreamModel.mjs", import.meta.url), "utf8");
  assert.ok(!model.includes('"harnesses"') && !model.includes('"shells"') && !model.includes("function plural("));
  assert.equal(terminationConsent({ harnesses: 1, shells: 0, agents: 0 }), "Closing it ends what stands in it: 1 harness terminated.");
  assert.equal(terminationConsent({ harnesses: 0, shells: 0, agents: 0 }), null, "nothing to consent to");
  for (const words of [terminationWords({ harnesses: 1, shells: 1, agents: 1 }), terminationConsent({ harnesses: 1, shells: 1, agents: 1 })]) {
    assert.doesNotMatch(words, /kill/i, "the word is terminate");
  }
});

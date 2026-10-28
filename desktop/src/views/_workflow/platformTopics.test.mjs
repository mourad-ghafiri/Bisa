/**
 * The topic list the designer offers is the engine's own, in its order:
 * read from `events::TOPICS` as the workspace load model reads the engine's
 * facts. Run with `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/platformTopics.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { PLATFORM_TOPICS } from "./platformTopics.mjs";

test("the platform topics are the engine's TOPICS, in the engine's order, each its own label", () => {
  const rust = readFileSync(new URL("../../../../crates/bisa-engine/src/events.rs", import.meta.url), "utf8");
  const from = rust.indexOf("pub const TOPICS: &[&str] = &[");
  assert.ok(from > 0, "the engine's constant is found");
  const body = rust.slice(from, rust.indexOf("];", from));
  const topics = [...body.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  assert.ok(topics.length > 60, `the engine names ${topics.length} topics`);
  assert.deepEqual(PLATFORM_TOPICS.map((t) => t.word), topics);
  for (const t of PLATFORM_TOPICS) assert.equal(t.label, t.word, "a topic is its own label: the machine's word");
  assert.equal(new Set(topics).size, topics.length, "no topic twice");
});

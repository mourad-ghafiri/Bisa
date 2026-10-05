/**
 * Every harness the Rust catalog compiles in or presets has a mark, and the
 * prefixed families fold to one. Reads `catalog.rs` as text, the way
 * `themes.test.mjs` mirrors the Rust theme list. Run with
 * `node --test desktop/src/ui/harnessMarkModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { MARK_IDS, markIdOf } from "./harnessMarkModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const catalog = readFileSync(join(here, "../../../crates/bisa-harness/src/catalog.rs"), "utf8");

function builtinIds() {
  const block = catalog.match(/pub const BUILTIN_IDS: &\[&str\] = &\[([\s\S]*?)\];/);
  assert.ok(block, "catalog.rs lost its BUILTIN_IDS list");
  return [...block[1].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]);
}

function presetIds() {
  return [...catalog.matchAll(/PresetHarness \{\s*id: "(preset:[a-z-]+)"/g)].map((m) => m[1]);
}

test("every built-in and every preset harness resolves to a mark, and the marks name nothing else", () => {
  const builtins = builtinIds();
  assert.ok(builtins.length >= 5, builtins.join(","));
  for (const id of builtins) assert.equal(markIdOf(id), id, `${id} wears its own mark`);
  const presets = presetIds();
  assert.ok(presets.length >= 1, "the catalog lost its presets");
  for (const id of presets) assert.ok(markIdOf(id), `${id} has a mark`);
  const covered = new Set([...builtins, ...presets.map((p) => p.slice("preset:".length))]);
  for (const mark of MARK_IDS) assert.ok(covered.has(mark), `${mark} is a harness the catalog knows`);
  assert.equal(new Set(MARK_IDS).size, MARK_IDS.length);
});

test("the three harnesses that speak ACP under an id of their own wear their own marks, never the protocol's plug", () => {
  const own = [["copilot", "CopilotMark"], ["grok", "GrokMark"], ["gemini", "GeminiMark"]];
  for (const [id] of own) {
    assert.ok(builtinIds().includes(id), `the catalog compiles ${id} in`);
    assert.equal(markIdOf(id), id);
  }
  // The generic route into an agent is still the plug, whatever the agent.
  assert.equal(markIdOf("acp:copilot"), "acp");
  assert.equal(markIdOf("acp:gemini"), "acp");
  const marks = readFileSync(join(here, "harnessMarks.tsx"), "utf8");
  for (const [id, component] of own) {
    assert.match(marks, new RegExp(`^  ${id}: ${component},$`, "m"), `${id} is drawn by ${component}`);
    assert.match(marks, new RegExp(`<Svg \\{\\.\\.\\.p\\} id="${id}">`), `${component} names the id it stands for`);
  }
});

test("the prefixed families fold to one mark and the unknown to none", () => {
  assert.equal(markIdOf("acp:zed"), "acp");
  assert.equal(markIdOf("custom:my-agent"), "custom");
  assert.equal(markIdOf("preset:goose"), "goose");
  assert.equal(markIdOf("preset:cursor-agent"), "cursor-agent");
  assert.equal(markIdOf("preset:something-else"), null, "a preset without a mark of its own is the terminal glyph");
  assert.equal(markIdOf("shell"), null);
  assert.equal(markIdOf(""), null);
  assert.equal(markIdOf(null), null);
});

/**
 * The Board's keys are the registry's, and the desktop's fallbacks are the
 * registry's defaults — read from the Rust source, so a default that moves
 * in `settings.rs` fails here rather than on the first frame before the node
 * answers. Run with `node --test desktop/src/views/_board/boardSettings.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { BOARD_ARCHIVED_KEY, BOARD_DEFAULTS, BOARD_DUE_SOON_KEY, BOARD_ENABLED_KEY, BOARD_KEYS, BOARD_WIP_KEY } from "./boardSettings.mjs";

const registry = readFileSync(new URL("../../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");

/** The `def!(` entry for a key: its kind line and its `json!(…)` default. */
function entry(key) {
  const at = registry.indexOf(`"${key}",`);
  assert.ok(at > 0, `${key} is registered`);
  // `def!("key", Kind, json!(default), S::…)` — on one line or spread over several.
  const block = registry.slice(at, registry.indexOf("S::", at));
  const m = /^"[a-z0-9_.]+",\s*([A-Za-z]+(?:\([^)]*\)|\s*\{[^}]*\})?),\s*json!\((.*)\),\s*$/s.exec(block.trim());
  assert.ok(m, `${key} has a kind and a default`);
  return { kind: `${m[1]},`, fallback: JSON.parse(m[2]) };
}

test("every Board key is a registry key under workstreams.board, and nothing else is", () => {
  assert.deepEqual([...BOARD_KEYS], [BOARD_ENABLED_KEY, BOARD_DUE_SOON_KEY, BOARD_WIP_KEY, BOARD_ARCHIVED_KEY]);
  for (const key of BOARD_KEYS) {
    assert.ok(key.startsWith("workstreams.board."), `${key} is a Board key`);
    entry(key);
  }
  const registered = [...registry.matchAll(/"(workstreams\.board\.[a-z_]+)",/g)].map((m) => m[1]);
  assert.deepEqual([...new Set(registered)].sort(), [...BOARD_KEYS].sort(), "the registry has no Board key the desktop forgot");
});

test("the fallbacks are the registry's defaults, kind for kind", () => {
  assert.deepEqual(entry(BOARD_ENABLED_KEY), { kind: "Bool,", fallback: BOARD_DEFAULTS.enabled });
  assert.deepEqual(entry(BOARD_ARCHIVED_KEY), { kind: "Bool,", fallback: BOARD_DEFAULTS.showArchived });
  const dueSoon = entry(BOARD_DUE_SOON_KEY);
  assert.equal(dueSoon.fallback, BOARD_DEFAULTS.dueSoonDays);
  assert.match(dueSoon.kind, /^Integer \{ min: 1, max: 30 \},$/, "due soon is a bounded whole number of days");
  const wip = entry(BOARD_WIP_KEY);
  assert.equal(wip.fallback, BOARD_DEFAULTS.wipLimit);
  assert.match(wip.kind, /^Integer \{ min: 0, max: 50 \},$/, "zero means no limit, so the floor is zero");
});

test("the defaults are frozen facts, not a place to write", () => {
  assert.ok(Object.isFrozen(BOARD_DEFAULTS));
  assert.ok(Object.isFrozen(BOARD_KEYS));
  assert.throws(() => {
    "use strict";
    BOARD_DEFAULTS.enabled = false;
  }, TypeError);
});

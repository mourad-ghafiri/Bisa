import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";
import { sourceFiles } from "./testWalk.mjs";

/**
 * Every `bisa.*` localStorage key the desktop writes is listed in
 * `docs/architecture/crates/desktop.md`, and nothing is listed there that the
 * code no longer writes. A key is per-viewer state; a person clearing one
 * should be able to find what it was for.
 */
const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");

function keysInCode() {
  const keys = new Set();
  for (const file of sourceFiles(join(root, "desktop/src"), (p) => /\.(ts|tsx|mjs)$/.test(p) && !/\.test\.mjs$/.test(p))) {
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(/"(bisa\.[a-z0-9.]+)"/g)) keys.add(m[1]);
  }
  return keys;
}

function keysInDoc() {
  const doc = readFileSync(join(root, "docs/architecture/crates/desktop.md"), "utf8");
  const keys = new Set();
  for (const m of doc.matchAll(/`(bisa\.[a-z0-9.]+)`/g)) keys.add(m[1]);
  return keys;
}

test("the desktop page lists every localStorage key, and only real ones", () => {
  const code = keysInCode();
  const doc = keysInDoc();
  assert.ok(code.size > 10, `found ${code.size} keys in code; the scanner is broken`);
  const undocumented = [...code].filter((k) => !doc.has(k)).sort();
  const phantom = [...doc].filter((k) => !code.has(k)).sort();
  assert.deepEqual(
    { undocumented, phantom },
    { undocumented: [], phantom: [] },
    `docs/architecture/crates/desktop.md and desktop/src disagree (${relative(root, here)})`,
  );
});

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";
import { sourceFiles } from "../testWalk.mjs";

/**
 * The kit's radii are tokens (`rounded-control`, `rounded-card`, Tailwind's
 * own `rounded` and `rounded-sm`), never a pixel literal: a corner spelled
 * spelled in pixels in one panel drifts from the token the moment the theme
 * changes it.
 */
const here = dirname(fileURLToPath(import.meta.url));
const src = join(here, "..");

test("no component spells a radius in pixels", () => {
  const offences = [];
  for (const file of sourceFiles(src, (p) => /\.(ts|tsx)$/.test(p) && !p.includes("node_modules") && !p.endsWith("types.gen.ts"))) {
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(/rounded(?:-[a-z]+)?-\[[0-9.]+px\]/g)) offences.push(`${relative(src, file)}: ${m[0]}`);
  }
  assert.deepEqual(offences, [], "use rounded-control / rounded-card / rounded / rounded-sm");
});

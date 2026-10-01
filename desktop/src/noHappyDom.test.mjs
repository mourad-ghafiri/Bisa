/**
 * `happy-dom` is a dev dependency for exactly one script —
 * `scripts/check-mermaid.mjs`, which needs a DOM to *parse* a diagram — and
 * for nothing in here. The desktop's rule is that models are tested and
 * components are not, and a DOM in a test is how that line blurs.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { sourceFiles } from "./testWalk.mjs";

const SRC = dirname(fileURLToPath(import.meta.url));

test("no test, model or component under src/ imports happy-dom or jsdom", () => {
  const offenders = [];
  for (const file of sourceFiles(SRC, (p) => /\.(mjs|ts|tsx)$/.test(p))) {
    const text = readFileSync(file, "utf8");
    if (/from\s+["'](happy-dom|jsdom)["']|require\(["'](happy-dom|jsdom)["']\)/.test(text)) {
      offenders.push(file.slice(SRC.length + 1));
    }
  }
  assert.deepEqual(offenders, []);
});

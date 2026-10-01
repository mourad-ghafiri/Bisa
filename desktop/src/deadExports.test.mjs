/**
 * Every exported value under `src/` is imported by another source file or
 * read by a test — an export nothing reaches is dead code, and dead code is
 * where the next bug hides unread. Type exports are not counted: a `.d.mts`
 * names shapes for the compiler, and an interface nobody imports costs
 * nothing at runtime. `main.tsx` is the entry point and imports, never
 * exports. A source guard, as `noHappyDom.test.mjs` is.
 *
 * Run with `node --test desktop/src/deadExports.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { sourceFiles } from "./testWalk.mjs";

const SRC = dirname(fileURLToPath(import.meta.url));
const isSource = (p) => /\.(ts|tsx|mjs)$/.test(p) && !p.endsWith(".d.mts") && !/\.test\.mjs$/.test(p) && !p.endsWith("types.gen.ts");
/** Exports the compiler reads rather than a caller: each with the reason it stays. */
const COMPILE_TIME = Object.freeze({
  "activity.ts: RENDERED": "the exhaustiveness check over every wire payload — `noUnusedLocals` refuses it unexported, and a caller would be wrong",
});
const escape = (name) => name.replace(/\$/g, "\\$");

/** The exported value names of one source: declarations and `export { … }` lists, types left out. */
function exportedValues(text) {
  const names = new Set();
  for (const m of text.matchAll(/^export (?:const|let|function|class|async function) ([A-Za-z_$][\w$]*)/gm)) names.add(m[1]);
  for (const m of text.matchAll(/^export \{([^}]*)\}/gm)) {
    for (const part of m[1].split(",")) {
      const word = part.trim();
      if (!word || word.startsWith("type ")) continue;
      names.add(word.split(/\s+as\s+/).pop());
    }
  }
  return names;
}

test("every exported value is imported by a source or read by a test", () => {
  const sources = sourceFiles(SRC, isSource).map((p) => [p, readFileSync(p, "utf8")]);
  const tests = sourceFiles(SRC, (p) => /\.test\.mjs$/.test(p)).map((p) => readFileSync(p, "utf8")).join("\n");
  const everything = sources.map(([, t]) => t).join("\n");
  const dead = [];
  for (const [file, text] of sources) {
    // The entry point imports and never exports: a reader, not a source of exports.
    if (file.endsWith("main.tsx")) continue;
    const others = everything.replace(text, "");
    for (const name of exportedValues(text)) {
      const re = new RegExp(`\\b${escape(name)}\\b`);
      const key = `${relative(SRC, file)}: ${name}`;
      if (!re.test(others) && !re.test(tests) && !(key in COMPILE_TIME)) dead.push(key);
    }
  }
  assert.deepEqual(dead, [], "an export nothing reaches: remove it, or unexport it when only its own file uses it");
});

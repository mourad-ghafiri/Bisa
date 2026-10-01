/**
 * A model is a `.mjs` beside a `.d.mts`: the first is what runs, the second
 * what the compiler believes. They are two files written by hand, so they
 * can disagree — and each way is a fault a person meets in the window:
 *
 * - a value the `.mjs` exports and the `.d.mts` does not declare cannot be
 *   imported by a component (`tsc` refuses it) — found only at compile time;
 * - a value the `.d.mts` declares and the `.mjs` does not export type-checks
 *   and is `undefined` when it runs.
 *
 * This holds the two equal, name by name, for every pair under `src/`. Types
 * and interfaces are the compiler's alone and are not counted. A source
 * guard, as `deadExports.test.mjs` is.
 *
 * Run with `node --test desktop/src/declarations.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { existsSync, readFileSync } from "node:fs";
import { dirname, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { sourceFiles } from "./testWalk.mjs";

const SRC = dirname(fileURLToPath(import.meta.url));

/** The names of one `export { a, b as c }` list, types left out. */
function listed(list) {
  const names = [];
  for (const part of list.split(",")) {
    const word = part.trim();
    if (!word || word.startsWith("type ")) continue;
    names.push(word.split(/\s+as\s+/).pop());
  }
  return names;
}

/** The values a model exports when it runs. */
export function runtimeExports(text) {
  const names = new Set();
  for (const m of text.matchAll(/^export (?:const|let|var|function\*?|class|async function\*?) ([A-Za-z_$][\w$]*)/gm)) names.add(m[1]);
  for (const m of text.matchAll(/^export \{([^}]*)\}/gm)) for (const name of listed(m[1])) names.add(name);
  return names;
}

/** The values a declaration file says the model exports. */
export function declaredValues(text) {
  const names = new Set();
  for (const m of text.matchAll(/^export (?:declare )?(?:const|let|var|function|class|abstract class) ([A-Za-z_$][\w$]*)/gm)) names.add(m[1]);
  for (const m of text.matchAll(/^export \{([^}]*)\}/gm)) for (const name of listed(m[1])) names.add(name);
  return names;
}

test("the readers tell a value from a type", () => {
  const model = "export const A = 1;\nexport function b() {}\nexport async function c() {}\nconst d = 2;\nexport { d, e as f };\nexport class G {}\n";
  assert.deepEqual([...runtimeExports(model)].sort(), ["A", "G", "b", "c", "d", "f"]);
  const declared =
    "export interface Shape { a: number }\nexport type Word = string;\nexport declare const A: number;\nexport declare function b(): void;\nexport function c(): Promise<void>;\nexport declare class G {}\nexport { type Shape as Other, d };\n";
  assert.deepEqual([...declaredValues(declared)].sort(), ["A", "G", "b", "c", "d"]);
});

test("every value a model exports is declared, and every value declared is exported", () => {
  const pairs = sourceFiles(SRC, (p) => p.endsWith(".d.mts"))
    .map((declaration) => [declaration.replace(/\.d\.mts$/, ".mjs"), declaration])
    .filter(([model]) => existsSync(model));
  assert.ok(pairs.length > 100, `the walk found ${pairs.length} models with a declaration; it is broken`);
  const undeclared = [];
  const unbacked = [];
  for (const [model, declaration] of pairs) {
    const runs = runtimeExports(readFileSync(model, "utf8"));
    const says = declaredValues(readFileSync(declaration, "utf8"));
    const name = relative(SRC, model);
    for (const value of runs) if (!says.has(value)) undeclared.push(`${name}: ${value}`);
    for (const value of says) if (!runs.has(value)) unbacked.push(`${name}: ${value}`);
  }
  assert.deepEqual(unbacked, [], "declared in the .d.mts and exported by nothing: it type-checks and is undefined when it runs");
  assert.deepEqual(undeclared, [], "exported by the .mjs and declared nowhere: no component can import it");
});

/**
 * A name the type generator numbered is nobody's to import. `types.gen.ts`
 * is written from the node's schema, and where one shape is reached two ways
 * the generator declares it twice — `Step` and `Step1`. The number is the
 * generator's bookkeeping: it moves when the schema's order does, so a file
 * that imported `ValueRef4` for a project came to hold an account the day a
 * field was added above it. The schema itself numbers nothing (the node's
 * generator refuses to); what is left is the TypeScript side's own, and no
 * hand-written file names it.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { sourceFiles } from "./testWalk.mjs";

const SRC = dirname(fileURLToPath(import.meta.url));
const GENERATED = join(SRC, "types.gen.ts");

/** Every name `types.gen.ts` declares. */
function declared() {
  const names = new Set();
  for (const m of readFileSync(GENERATED, "utf8").matchAll(/^export (?:type|interface) (\w+)/gm)) names.add(m[1]);
  return names;
}

/** The declared names that are another declared name with a number after it. */
function numbered(names) {
  return [...names].filter((name) => {
    const base = name.replace(/\d+$/, "");
    return base !== name && names.has(base);
  });
}

test("the generated types are there to be read, and a numbered name is told from one with digits of its own", () => {
  const names = declared();
  assert.ok(names.size > 500, `types.gen.ts declares ${names.size} names: the guard would hold nothing`);
  assert.deepEqual(numbered(new Set(["Step", "Step1", "Sha256", "Goal"])), ["Step1"]);
});

test("the schema's own names carry no number: a value reference is named after what it carries", () => {
  const names = declared();
  for (const name of ["AssigneeOrInput", "ProjectIdOrInput", "AccountIdOrInput", "Uint64OrInput", "StringOrInput", "MobilePlatform", "InstallPlatform"]) {
    assert.ok(names.has(name), `${name} is declared`);
  }
  for (const gone of ["ValueRef", "ValueRef2", "Platform", "Platform2"]) {
    assert.ok(!names.has(gone), `${gone} named several types at once`);
  }
});

test("no hand-written file names a type the generator numbered", () => {
  const taken = numbered(declared());
  assert.ok(taken.length > 0, "the generator numbers a shape it reaches twice; none found means the pattern no longer matches");
  const word = new RegExp(`\\b(?:${taken.join("|")})\\b`);
  const offenders = [];
  for (const file of sourceFiles(SRC, (p) => /\.(ts|tsx|mts|mjs)$/.test(p) && p !== GENERATED && !p.endsWith("generatedNames.test.mjs"))) {
    const found = readFileSync(file, "utf8").match(word);
    if (found) offenders.push(`${file.slice(SRC.length + 1)}: ${found[0]}`);
  }
  assert.deepEqual(offenders, []);
});

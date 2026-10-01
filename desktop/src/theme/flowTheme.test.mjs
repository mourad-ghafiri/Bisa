/**
 * The canvas theme, enforced.
 *
 * `flow.css` redirects xyflow's `--xy-*` variables to the role contract. A
 * variable name that xyflow does not read falls back to its own greys
 * silently — the canvas looks *almost* themed, and nobody files that bug. So
 * every `--xy-*` set here must appear in the installed stylesheet, and every
 * value must be a role, a `color-mix` of roles, or a plain size or `none`.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ours = readFileSync(join(HERE, "flow.css"), "utf8");
const theirs = readFileSync(join(HERE, "../../node_modules/@xyflow/react/dist/style.css"), "utf8");

function declarations() {
  return [...ours.matchAll(/(--xy-[a-z0-9-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]);
}

test("every --xy variable we set is one xyflow reads", () => {
  const known = new Set([...theirs.matchAll(/--xy-[a-z0-9-]+/g)].map((m) => m[0]));
  const unknown = declarations()
    .map(([name]) => name)
    .filter((n) => !known.has(n));
  assert.deepEqual(unknown, [], "xyflow's stylesheet does not read these");
  assert.ok(declarations().length > 20, "the theme covers the canvas");
});

test("no --xy value names a colour; every one is a role", () => {
  for (const [name, value] of declarations()) {
    const roles = [...value.matchAll(/var\(--color-[a-z0-9-]+\)/g)].length;
    const literalColour = /#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(|oklch\(\s*[0-9]/i.test(value);
    assert.ok(!literalColour, `${name} names a colour: ${value}`);
    const plain = /^(none|transparent|[0-9.]+(px)?|[0-9.]+ [0-9.]+|[0-9.]+px (solid|dashed) var\(--color-[a-z0-9-]+\)|0 0 0 [0-9]+px var\(--color-[a-z0-9-]+\))$/.test(value);
    assert.ok(roles > 0 || plain, `${name} is neither a role nor a plain size: ${value}`);
  }
});

test("the roles the canvas leans on are in the contract", () => {
  const tokens = readFileSync(join(HERE, "tokens.css"), "utf8");
  const region = tokens.match(/@roles:start\s*\*\/([\s\S]*?)\/\*\s*@roles:end/);
  const contract = new Set([...region[1].matchAll(/(--color-[a-z0-9-]+)\s*:/g)].map((m) => m[1]));
  for (const [, value] of declarations()) {
    for (const m of value.matchAll(/var\((--color-[a-z0-9-]+)\)/g)) {
      assert.ok(contract.has(m[1]), `${m[1]} is not a role every theme answers`);
    }
  }
});

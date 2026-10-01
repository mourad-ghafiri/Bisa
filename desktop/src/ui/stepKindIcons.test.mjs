/**
 * Every step kind the wire can carry has a glyph, and an unknown one has
 * the shared stand-in — so a step from another build never reaches React
 * as an `undefined` element type. Run with
 * `node --test desktop/src/ui/stepKindIcons.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const gen = readFileSync(new URL("../types.gen.ts", import.meta.url), "utf8");
const icons = readFileSync(new URL("./icons.ts", import.meta.url), "utf8");

function kindsOfStepKind() {
  const start = gen.indexOf("export type StepKind =");
  assert.ok(start >= 0, "StepKind is generated");
  const end = gen.indexOf("\nexport ", start + 1);
  return [...gen.slice(start, end).matchAll(/kind: "([a-z_]+)"/g)].map((m) => m[1]);
}

test("the icon table names every StepKind and nothing else", () => {
  const kinds = kindsOfStepKind();
  assert.ok(kinds.length >= 10, `the union was read: ${kinds}`);
  const start = icons.indexOf("export const STEP_KIND_ICON");
  const body = icons.slice(start, icons.indexOf("\n};", start));
  const keys = [...body.matchAll(/^\s+([a-z_]+): /gm)].map((m) => m[1]);
  assert.deepEqual([...keys].sort(), [...new Set(kinds)].sort());
});

test("an unknown kind is drawn with the stand-in, never as undefined", () => {
  assert.match(icons, /export function stepKindIcon\(/);
  assert.match(icons, /STEP_KIND_ICON\[kind as StepKind\["kind"\]\] \?\? STEP_ICON_FALLBACK/);
});

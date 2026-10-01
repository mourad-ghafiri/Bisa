/**
 * An indicator earns its place after a beat. Run with
 * `node --test desktop/src/ui/loadingModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { INDICATOR_DELAY_MS, beatMs, indicatorDue, placeholderFill } from "./loadingModel.mjs";

test("the beat is one a person reads as instant — between a tenth and three tenths of a second", () => {
  assert.ok(INDICATOR_DELAY_MS >= 100 && INDICATOR_DELAY_MS <= 300, String(INDICATOR_DELAY_MS));
});

test("an indicator is due once the beat has passed since it mounted, and at once with no delay", () => {
  assert.equal(indicatorDue(1000, 1000), false);
  assert.equal(indicatorDue(1000, 1000 + INDICATOR_DELAY_MS - 1), false);
  assert.equal(indicatorDue(1000, 1000 + INDICATOR_DELAY_MS), true);
  assert.equal(indicatorDue(1000, 5000), true);
  assert.equal(indicatorDue(1000, 1000, 0), true);
  assert.equal(indicatorDue(1000, 1050, 100), false);
  assert.equal(indicatorDue(1000, 1100, 100), true);
});

test("inside a surface that just opened the beat is none; in a drawn page it is the kit's", () => {
  // A dialog or a popover has already appeared: a blank body for the beat
  // would be the flash the beat exists to avoid.
  assert.equal(beatMs(true), 0);
  assert.equal(beatMs(false), INDICATOR_DELAY_MS);
  assert.equal(indicatorDue(1000, 1000, beatMs(true)), true, "at once inside a surface");
  assert.equal(indicatorDue(1000, 1000, beatMs(false)), false, "after the beat in a page");
});

test("every surface that opens onto a read provides the immediate beat, and the hook reads it", () => {
  const source = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  for (const surface of ["./Dialog.tsx", "./Popover.tsx"]) {
    assert.ok(source(surface).includes("<ImmediateIndicators>"), `${surface} opens onto its status`);
  }
  assert.ok(source("./useShowAfter.ts").includes("beatMs("), "the hook takes its beat from the model");
});

test("a placeholder holds its place unseen until the beat, then pulses", () => {
  const waiting = placeholderFill(false);
  assert.equal(waiting, "invisible", "the box is kept, nothing is painted");
  assert.ok(!waiting.includes("bg-") && !waiting.includes("motion-pulse"), "no fill and no pulse before the beat");
  const due = placeholderFill(true);
  assert.ok(due.includes("motion-pulse") && due.includes("bg-surface-2"), due);
  assert.ok(!due.includes("invisible"));
});

test("no pending piece of the kit is immediate: every exported one waits the beat, and the immediate fill is the file's own", () => {
  const text = readFileSync(new URL("./Skeleton.tsx", import.meta.url), "utf8");
  const exported = [...text.matchAll(/^export function (\w+)\(/gm)].map((m) => m[1]);
  assert.deepEqual(exported, ["Skeleton", "SkeletonRows", "Pending"]);
  for (const name of exported) {
    const from = text.indexOf(`export function ${name}(`);
    const next = text.indexOf("\nexport function ", from + 1);
    const body = text.slice(from, next === -1 ? undefined : next);
    assert.ok(body.includes("useShowAfter()"), `${name} waits the beat`);
  }
  assert.ok(/^function Fill\(/m.test(text) && !text.includes("export function Fill"), "the immediate rectangle is never exported");
  assert.ok(text.includes("placeholderFill(due)"), "the block is filled by the model's rule");
  assert.ok(!text.includes('"motion-pulse block rounded-control bg-surface-2"'), "no block is born filled");
  const kit = readFileSync(new URL("./index.ts", import.meta.url), "utf8");
  assert.ok(!/\bFill\b/.test(kit), "and the kit does not hand it out");
});

test("nothing outside the kit draws a pulsing block of its own", () => {
  const src = join(fileURLToPath(new URL(".", import.meta.url)), "..");
  // A pulse that is not a placeholder box, each with its reason.
  const allowed = new Map([
    ["ui/ThinkingBlock.tsx", "the streaming caret; a two-pixel bar, not a box"],
    ["views/_work/LifecycleStepper.tsx", "the live step's dot; a six-pixel ring, not a box"],
    ["views/_workflow/LibraryCard.tsx", "a library card's live state: a six-pixel dot while a goal runs the workflow, not a box"],
    ["ui/Skeleton.tsx", "the kit's own, behind the beat"],
  ]);
  const files = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const p = join(dir, entry.name);
      if (entry.isDirectory()) walk(p);
      else if (p.endsWith(".tsx")) files.push(p);
    }
  };
  walk(src);
  const strays = files
    .map((p) => p.slice(src.length + 1))
    .filter((rel) => !allowed.has(rel))
    .filter((rel) => /motion-pulse|animate-pulse/.test(readFileSync(join(src, rel), "utf8")));
  assert.deepEqual(strays, [], "a placeholder is the kit's `Skeleton`, which waits the beat — never a pulsing box of a screen's own");
});

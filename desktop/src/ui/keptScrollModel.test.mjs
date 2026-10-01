import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";
import { KEEP_ATTR, parsePlace, placeOf, restoreStep } from "./keptScrollModel.mjs";

const box = (scrollHeight, clientHeight = 600, scrollWidth = 800, clientWidth = 800) => ({ scrollHeight, clientHeight, scrollWidth, clientWidth });

test("a restore waits for a rendering that is still growing, and is done once the kept place fits", () => {
  const kept = { top: 2400, left: 0 };
  // Just mounted: one page of a PDF is in, the place is far below it.
  assert.deepEqual(restoreStep(kept, box(1000)), { top: 400, left: 0, done: false }, "as far as it goes for now");
  assert.deepEqual(restoreStep(kept, box(2900)), { top: 2300, left: 0, done: false });
  assert.deepEqual(restoreStep(kept, box(3000)), { top: 2400, left: 0, done: true }, "exactly reachable");
  assert.deepEqual(restoreStep(kept, box(9000)), { top: 2400, left: 0, done: true });
});

test("both axes are kept — a wide sheet comes back to its column too", () => {
  const kept = { top: 100, left: 1500 };
  assert.deepEqual(restoreStep(kept, box(2000, 600, 1600, 800)), { top: 100, left: 800, done: false });
  assert.deepEqual(restoreStep(kept, box(2000, 600, 3000, 800)), { top: 100, left: 1500, done: true });
});

test("a place read back from a memory is two offsets that are not negative, else nothing", () => {
  assert.deepEqual(parsePlace({ top: 120, left: 0 }), { top: 120, left: 0 });
  assert.deepEqual(parsePlace({ top: 12.6, left: 3 }), { top: 13, left: 3 });
  assert.equal(parsePlace({ top: 0, left: 0 }), null, "the origin is no place to keep");
  for (const raw of [null, undefined, "120", 120, [120, 0], {}, { top: 120 }, { top: "120", left: 0 }, { top: -1, left: 0 }, { top: Number.NaN, left: 0 }]) assert.equal(parsePlace(raw), null);
});

test("the hook keeps a document's last scroll under that document, not the next one", () => {
  const hook = readFileSync(new URL("./useKeptScroll.ts", import.meta.url), "utf8");
  assert.ok(hook.includes("let own = keptRef.current;") && hook.includes("own.write(name, place)"), "the writer is taken when the effect starts");
  assert.ok(!hook.includes("keptRef.current.write("), "a cleanup never writes through the ref, which by then is the next document's");
});

test("nothing kept is done at once, and a scrollport at its origin keeps nothing", () => {
  for (const nothing of [null, undefined, { top: 0, left: 0 }]) assert.deepEqual(restoreStep(nothing, box(5000)), { top: 0, left: 0, done: true });
  assert.equal(placeOf(0, 0), null);
  assert.equal(placeOf(0.4, 0), null);
  assert.deepEqual(placeOf(120.6, 3.2), { top: 121, left: 3 });
  assert.deepEqual(placeOf(-5, 40), { top: 0, left: 40 }, "a rubber-band overscroll is the origin");
  assert.deepEqual(restoreStep({ top: -10, left: 0 }, box(5000)), { top: 0, left: 0, done: true });
});

test("every scrolling viewer marks its scrollport, so a new one cannot forget its place", () => {
  assert.equal(KEEP_ATTR, "data-scroll-keep");
  const dir = new URL("./artifact/", import.meta.url);
  const offences = [];
  for (const name of readdirSync(dir).filter((f) => /View\.tsx$/.test(f))) {
    const source = readFileSync(new URL(name, dir), "utf8");
    // Each opening tag that scrolls: `overflow-auto` in its class list.
    for (const tag of source.match(/<[a-zA-Z][^<>]*\boverflow-(?:auto|y-auto|x-auto)\b[^<>]*>/g) ?? []) {
      if (!tag.includes(KEEP_ATTR)) offences.push(`${name}: ${tag.slice(0, 90)}`);
    }
  }
  assert.deepEqual(offences, [], `mark the scrollport with ${KEEP_ATTR}="<name>"`);
});

import { strict as assert } from "node:assert";
import { test } from "node:test";
import { LIBRARY_ITEMS, buildLibrary } from "./index.mjs";

const VECTOR = new Set(["rectangle", "ellipse", "diamond", "arrow", "line", "text", "frame"]);

test("every shape has a unique id, a name, and a skeleton of vector elements — never an image", () => {
  const ids = LIBRARY_ITEMS.map((i) => i.id);
  assert.equal(new Set(ids).size, ids.length);
  assert.ok(ids.length >= 20, `${ids.length} shapes ship`);
  for (const item of LIBRARY_ITEMS) {
    assert.ok(item.name.trim().length > 0, `${item.id} has a name`);
    const skeleton = item.skeleton();
    assert.ok(skeleton.length > 0, `${item.id} draws something`);
    const seen = new Set();
    for (const e of skeleton) {
      assert.ok(VECTOR.has(e.type), `${item.id}: ${e.type} is a vector type`);
      assert.ok(!seen.has(e.id), `${item.id}: ${e.id} twice`);
      seen.add(e.id);
      assert.ok(!JSON.stringify(e).includes("http"), `${item.id}: nothing reaches outside`);
    }
  }
});

test("the library is built with the converter lent to it, one published item per shape", () => {
  const items = buildLibrary((skeleton) => skeleton.map((e) => ({ ...e, converted: true })), 1_700_000_000_000);
  assert.equal(items.length, LIBRARY_ITEMS.length);
  for (const item of items) {
    assert.ok(item.id.startsWith("bisa-"));
    assert.equal(item.status, "published");
    assert.equal(item.created, 1_700_000_000_000);
    assert.ok(item.elements.every((e) => e.converted === true));
  }
});

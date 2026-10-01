import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { TEMPLATES, templateOf } from "./index.mjs";
import { PALETTE, arrow, box, note } from "./grid.mjs";

const VECTOR = new Set(["rectangle", "ellipse", "diamond", "arrow", "line", "text", "frame"]);

test("every template has a unique id, words, and a skeleton of vector elements whose arrows bind to boxes in it", () => {
  const ids = TEMPLATES.map((tpl) => tpl.id);
  assert.equal(new Set(ids).size, ids.length, "no id twice");
  assert.equal(ids[0], "empty", "Empty first");
  for (const tpl of TEMPLATES) {
    assert.ok(tpl.label.trim().length > 0, `${tpl.id} has words`);
    assert.ok(tpl.blurb.trim().length > 0 && tpl.blurb !== tpl.label, `${tpl.id} says when to reach for it`);
    const skeleton = tpl.skeleton();
    if (tpl.id === "empty") {
      assert.deepEqual(skeleton, []);
      continue;
    }
    assert.ok(skeleton.length >= 3, `${tpl.id} draws something`);
    const elementIds = skeleton.map((e) => e.id);
    assert.equal(new Set(elementIds).size, elementIds.length, `${tpl.id}: no element id twice`);
    for (const e of skeleton) {
      assert.ok(VECTOR.has(e.type), `${tpl.id}: ${e.type} is a vector type`);
      assert.ok(typeof e.id === "string" && e.id.length > 0, `${tpl.id}: every element has an id`);
      if (e.type !== "frame") assert.ok(Number.isFinite(e.x) && Number.isFinite(e.y), `${tpl.id}: ${e.id} has a place`);
      if (e.type === "arrow") {
        assert.ok(elementIds.includes(e.start.id) && elementIds.includes(e.end.id), `${tpl.id}: ${e.id} binds to boxes in the skeleton`);
      }
      if (e.type === "frame") for (const child of e.children) assert.ok(elementIds.includes(child), `${tpl.id}: frame ${e.id} names ${child}`);
      assert.ok(!JSON.stringify(e).includes("http"), `${tpl.id}: nothing reaches outside`);
    }
  }
});

test("every word a template draws comes through the catalog — no English is written in the builders", () => {
  const source = readFileSync(new URL("./index.mjs", import.meta.url), "utf8");
  const literals = [...source.matchAll(/"([^"\n]*)"/g)].map((m) => m[1]);
  const allowed = /^([a-z0-9-]*|#[0-9a-f]{6}|draw-[a-z0-9-]+|TLS|1 · n|…|◦   ◦   ◦|ellipse|diamond|rectangle|line|text|frame|arrow|solid|dashed|\.\.\/\.\.\/i18n\/l10n\.mjs|\.\/grid\.mjs)$/;
  const bare = literals.filter((s) => !allowed.test(s));
  assert.deepEqual(bare, [], "a literal a person reads must be a t() id");
});

test("a template is picked by id and an unknown one is Empty", () => {
  assert.equal(templateOf("kanban").id, "kanban");
  assert.equal(templateOf("nothing").id, "empty");
  assert.equal(templateOf(null).id, "empty");
});

test("the grid's words draw boxes, bound arrows and warm notes", () => {
  const a = box("a", 0, 0, "A");
  const b = box("b", 300, 0, "B", { type: "diamond", fill: PALETTE.red });
  assert.equal(a.type, "rectangle");
  assert.equal(a.label.text, "A");
  assert.equal(a.fillStyle, "solid");
  assert.equal(b.type, "diamond");
  assert.equal(b.backgroundColor, PALETTE.red);
  const ab = arrow("ab", a, b, "to");
  assert.deepEqual([ab.start, ab.end], [{ id: "a" }, { id: "b" }]);
  assert.equal(ab.label.text, "to");
  assert.ok(ab.x >= 0 && ab.width > 0);
  const n = note("n", 0, 0, "hello");
  assert.equal(n.backgroundColor, PALETTE.yellow);
  assert.equal(n.label.text, "hello");
});

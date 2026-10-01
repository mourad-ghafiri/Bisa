/**
 * A template's preview is read off its skeleton (19 — Drawings): boxes as
 * boxes with their words, arrows between their boxes' centres, frames around
 * what they name, a viewBox with a margin — and nothing for *Empty*. Run
 * with `node --test desktop/src/draw/templates/preview.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { templatePreview } from "./preview.mjs";
import { TEMPLATES } from "./index.mjs";
import { arrow, box, frame, title } from "./grid.mjs";

test("boxes, words, a bound arrow and a frame become shapes inside a padded viewBox", () => {
  const a = box("a", 0, 0, "A");
  const b = box("b", 300, 0, "B", { type: "diamond" });
  const p = templatePreview([title("t", 0, -60, "Words"), a, b, arrow("ab", a, b, "to"), frame("f", "F", ["a", "b"])]);
  const kinds = p.shapes.map((s) => s.kind);
  assert.equal(kinds[0], "frame", "a frame goes under what it holds");
  assert.ok(kinds.includes("rect") && kinds.includes("diamond") && kinds.includes("line"));
  const line = p.shapes.find((s) => s.kind === "line");
  assert.deepEqual([line.x1, line.y1, line.x2, line.y2], [90, 40, 390, 40], "centre to centre");
  assert.equal(line.arrow, true);
  const words = p.shapes.filter((s) => s.kind === "text").map((s) => s.text);
  assert.deepEqual(words.sort(), ["A", "B", "Words", "to"]);
  const fr = p.shapes[0];
  assert.ok(fr.x < 0 && fr.y < 0 && fr.w > 480 && fr.h > 80, "the frame wraps its children with an inset");
  assert.ok(p.viewBox.x <= fr.x - 40 + 1e-9 && p.viewBox.w > fr.w, "the viewBox pads the picture");
});

test("every template previews to finite shapes, and Empty to none", () => {
  for (const tpl of TEMPLATES) {
    const p = templatePreview(tpl.skeleton());
    if (tpl.id === "empty") {
      assert.deepEqual(p.shapes, []);
      continue;
    }
    assert.ok(p.shapes.length >= 3, `${tpl.id} shows something`);
    assert.ok(p.viewBox.w > 0 && p.viewBox.h > 0);
    for (const s of p.shapes) {
      for (const [k, v] of Object.entries(s)) {
        if (typeof v === "number") assert.ok(Number.isFinite(v), `${tpl.id}: ${s.kind}.${k} is a number`);
      }
    }
  }
});

test("a text's first line only, and an unbound arrow keeps its own place", () => {
  const p = templatePreview([{ id: "t", type: "text", x: 10, y: 10, text: "first\nsecond", fontSize: 20 }, { id: "l", type: "line", x: 0, y: 0, width: 100, height: 0 }]);
  assert.equal(p.shapes.find((s) => s.kind === "text").text, "first");
  const l = p.shapes.find((s) => s.kind === "line");
  assert.deepEqual([l.x1, l.y1, l.x2, l.y2, l.arrow], [0, 0, 100, 0, false]);
});

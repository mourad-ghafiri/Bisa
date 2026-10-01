import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { BODY, FOOTER, HEADER, PANEL } from "./dialogLayout.mjs";

const has = (classes, name) => classes.split(/\s+/).includes(name);

test("a footer wraps a row that cannot fit, so no button is ever cut off at the panel's edge", () => {
  assert.ok(has(FOOTER, "flex") && has(FOOTER, "flex-wrap"), "the row breaks rather than overflowing");
  assert.ok(has(FOOTER, "justify-end"), "actions sit on the right");
  assert.ok(!has(FOOTER, "flex-nowrap") && !has(FOOTER, "overflow-hidden"));
});

test("the body is the scrollport: a long body scrolls under a header and a footer that stay put", () => {
  assert.ok(has(PANEL, "flex") && has(PANEL, "flex-col"), "the panel is a column");
  assert.ok(has(PANEL, "max-h-[84vh]"), "capped at the viewport");
  assert.ok(!has(PANEL, "overflow-y-auto") && !has(PANEL, "overflow-auto"), "the panel itself never scrolls its buttons away");
  assert.ok(has(BODY, "overflow-y-auto") && has(BODY, "flex-1") && has(BODY, "min-h-0"), "the body takes what is left and scrolls");
  assert.ok(has(HEADER, "shrink-0") && has(FOOTER, "shrink-0"), "neither end gives up its height");
});

test("every dialog of the kit is drawn from these classes — none lays a footer out by hand", () => {
  const source = readFileSync(new URL("./Dialog.tsx", import.meta.url), "utf8") + readFileSync(new URL("./ChoiceDialog.tsx", import.meta.url), "utf8");
  assert.equal((source.match(/<footer\b/g) ?? []).length, 0, "a footer is `DialogFooter`, never a bare <footer>");
  assert.ok(!/justify-end/.test(source), "the alignment is the layout module's, in one place");
  assert.ok(source.includes("DialogFooter"));
});

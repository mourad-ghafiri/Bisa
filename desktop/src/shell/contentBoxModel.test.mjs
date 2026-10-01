import { strict as assert } from "node:assert";
import { test } from "node:test";
import { maximizedStyle, restoreOnEscape } from "./contentBoxModel.mjs";

test("a maximized panel is fixed at the content box in whole pixels, and nothing for no box", () => {
  assert.deepEqual(maximizedStyle({ left: 244.4, top: 40, width: 1035.6, height: 700.2 }), { position: "fixed", left: 244, top: 40, width: 1036, height: 700 });
  assert.equal(maximizedStyle(null), null);
  assert.equal(maximizedStyle({ left: 0, top: 0, width: 0, height: 10 }), null, "an unmeasured box is no box");
});

test("Escape restores only when the canvas has nothing of its own to cancel", () => {
  assert.equal(restoreOnEscape(null), true);
  assert.equal(restoreOnEscape({ selectedElementIds: {} }), true);
  assert.equal(restoreOnEscape({ selectedElementIds: { a: false } }), true);
  assert.equal(restoreOnEscape({ selectedElementIds: { a: true } }), false);
  assert.equal(restoreOnEscape({ editingTextElement: { id: "t" } }), false);
  assert.equal(restoreOnEscape({ editingLinearElement: { id: "l" } }), false);
  assert.equal(restoreOnEscape({ openMenu: "canvas" }), false);
  assert.equal(restoreOnEscape({ openSidebar: { name: "library" } }), false);
});

/**
 * What a document's view reads back as (`docViewModel.mjs`). Run with
 * `node --test desktop/src/views/_workbench/docViewModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { DOC_MODE, EDITOR_VIEW, PAGE_SCROLL, editorViewValue, placeValue, scrollName } from "./docViewModel.mjs";

test("an editor's view is handed back as the object it was kept as", () => {
  const view = { cursorState: [{ position: { lineNumber: 12, column: 3 } }], viewState: { scrollTop: 480 }, contributionsState: {} };
  assert.equal(editorViewValue(view), view, "by identity: the editor is handed what was kept");
  assert.deepEqual(editorViewValue(JSON.parse(JSON.stringify(view))), view, "and the same after a restart");
});

test("what could never have been an editor's view reads back as nothing", () => {
  for (const raw of [null, undefined, "", "{}", 12, true, [], [{ viewState: {} }]]) {
    assert.equal(editorViewValue(raw), null, JSON.stringify(raw) ?? String(raw));
  }
});

test("a place reads back as the value that was kept, so a viewer is handed the same one between renders", () => {
  const kept = { top: 120, left: 0 };
  assert.equal(placeValue(kept), kept);
  assert.deepEqual(placeValue({ top: 12.6, left: 0 }), { top: 13, left: 0 }, "whole pixels");
});

test("what is no place reads back as nothing", () => {
  for (const raw of [null, undefined, "120", 120, [], {}, { top: 120 }, { top: -1, left: 0 }, { top: "12", left: 0 }, { top: 0, left: 0 }]) {
    assert.equal(placeValue(raw), null, JSON.stringify(raw) ?? String(raw));
  }
});

test("a document's view is kept under names that cannot meet", () => {
  const names = [EDITOR_VIEW, PAGE_SCROLL, DOC_MODE, scrollName("page"), scrollName("editor"), scrollName("mode")];
  assert.equal(new Set(names).size, names.length, "a scrollport named like a slot is still its own");
  assert.equal(scrollName("sheet"), "scroll:sheet");
});

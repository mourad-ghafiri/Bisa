/**
 * Where the Draw panel floats while the Notes panel floats too. Run with
 * `node --test desktop/src/draw/besideModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { INSET, besideOffset } from "./besideModel.mjs";

test("beside the Notes panel when the window holds both — never on it", () => {
  assert.equal(besideOffset({ notesWidth: 560, drawWidth: 720, viewport: 1440 }), 560 + INSET);
  assert.equal(besideOffset({ notesWidth: 560, drawWidth: 720, viewport: 560 + 720 + 3 * INSET }), 560 + INSET, "exactly enough room is room");
});

test("in its own corner when the Notes panel is not floating, or the window cannot hold both", () => {
  assert.equal(besideOffset({ notesWidth: null, drawWidth: 720, viewport: 1440 }), 0);
  assert.equal(besideOffset({ notesWidth: 560, drawWidth: 720, viewport: 1024 }), 0);
  assert.equal(besideOffset({ notesWidth: 560, drawWidth: 720, viewport: 0 }), 0, "before the window is measured");
});

test("the panels wire the rule: Notes says its width, Draw stands beside it through a variable, its pinned style untouched", () => {
  const notes = readFileSync(new URL("../notes/NoteOverlay.tsx", import.meta.url), "utf8");
  const draw = readFileSync(new URL("./DrawOverlay.tsx", import.meta.url), "utf8");
  assert.ok(notes.includes('usePublishFloatingPanel("notes", open && !fixed ? width : null)'), "the Notes panel says its width while it floats");
  assert.ok(draw.includes('useFloatingPanelWidth("notes")') && draw.includes("besideOffset("), "the Draw panel reads it and asks the rule");
  assert.ok(draw.includes("mr-[var(--draw-beside,0px)]") && draw.includes("style={fixed ?? { width, height }}"), "beside through a variable; the size style stays as pinned");
});

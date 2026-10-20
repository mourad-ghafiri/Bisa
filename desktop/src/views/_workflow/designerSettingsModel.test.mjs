import test from "node:test";
import assert from "node:assert/strict";
import { AUTOSAVE_BOUNDS, DESIGNER_DEFAULTS, GRID_BOUNDS, VALIDATE_DELAY_MS, designerSettingsOf } from "./designerSettingsModel.mjs";

const setting = (key, value) => ({ key, value });

test("the designer's settings are read from the registry, held to their bounds, and the defaults stand for what is missing or ill-typed", () => {
  assert.deepEqual(designerSettingsOf(null), DESIGNER_DEFAULTS);
  assert.deepEqual(designerSettingsOf([]), DESIGNER_DEFAULTS);
  const read = designerSettingsOf([setting("workflow.designer.snap", false), setting("workflow.designer.grid", 8), setting("workflow.designer.minimap", true), setting("workflow.autosave.delay_ms", "1500")]);
  assert.deepEqual(read, { snap: false, grid: 8, minimap: true, autosaveDelayMs: 1500 }, "a number typed as text counts");
  const wild = designerSettingsOf([setting("workflow.designer.grid", 1000), setting("workflow.autosave.delay_ms", 1), setting("workflow.designer.snap", "yes")]);
  assert.equal(wild.grid, GRID_BOUNDS.max);
  assert.equal(wild.autosaveDelayMs, AUTOSAVE_BOUNDS.min);
  assert.equal(wild.snap, DESIGNER_DEFAULTS.snap, "a word is no switch");
  assert.equal(designerSettingsOf([setting("workflow.designer.grid", Number.NaN)]).grid, DESIGNER_DEFAULTS.grid);
  assert.ok(VALIDATE_DELAY_MS > 0 && VALIDATE_DELAY_MS < AUTOSAVE_BOUNDS.max);
  assert.ok(GRID_BOUNDS.min <= DESIGNER_DEFAULTS.grid && DESIGNER_DEFAULTS.grid <= GRID_BOUNDS.max);
  assert.ok(AUTOSAVE_BOUNDS.min <= DESIGNER_DEFAULTS.autosaveDelayMs && DESIGNER_DEFAULTS.autosaveDelayMs <= AUTOSAVE_BOUNDS.max);
});

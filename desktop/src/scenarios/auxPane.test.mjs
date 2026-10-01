/**
 * The Details pane's width (ide/18 §The Browser pane): its bounds are the
 * model's — a share of the room `App.tsx` measures — and the pane spells no
 * ceiling of its own. Source assertions, as `conversations.test.mjs` makes
 * them — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/auxPane.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("the pane's edge stops at the room's share, read from one model", () => {
  const pane = src("../shell/AuxPane.tsx");
  assert.ok(pane.includes("auxBounds(available)") && pane.includes("shownWidth(stored, bounds)"), "the bounds and the drawn width are the model's");
  assert.ok(!/const MAX\s*=|const MIN\s*=|const DEFAULT_WIDTH\s*=/.test(pane), "no number of its own");
  assert.ok(pane.includes("min={bounds.min}") && pane.includes("max={bounds.max}"), "the handle takes the room's bounds");
  const app = src("../App.tsx");
  assert.ok(app.includes("available={contentWidth}"), "the measured column is handed down");
  assert.ok(app.includes("setContentWidth(entry?.contentRect.width ?? 0)"), "one observer");
  assert.ok(app.includes("contentWidth > 0 && contentWidth < SINGLE_COLUMN_AT"), "the one-column rule reads the same number");
  const model = src("../shell/auxPaneModel.mjs");
  assert.ok(model.includes("AUX_MAX_SHARE = 0.7"), "seven tenths");
});

import test from "node:test";
import assert from "node:assert/strict";
import { DEFAULT_TAB, GOAL_TABS, GOAL_TAB_LABEL, tabOf } from "./goalTabs.mjs";

test("progress is first and the default; unknown values fall back to it", () => {
  assert.deepEqual([...GOAL_TABS], ["progress", "conversation", "workflow"]);
  assert.equal(DEFAULT_TAB, "progress");
  for (const t of GOAL_TABS) {
    assert.equal(tabOf(t), t);
    assert.ok(GOAL_TAB_LABEL[t]);
  }
  for (const bad of [null, undefined, "", "details", "about"]) assert.equal(tabOf(bad), "progress");
});

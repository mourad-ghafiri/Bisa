/**
 * Where a tab opened beside the screen is at home. Run with
 * `node --test desktop/src/shell/browserDoorsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { followCentre, paneToggle, screenHome } from "./browserDoorsModel.mjs";

test("the person's door to the Browser pane hides it while it shows, shows it while a tab is in sight, and opens one when there is none", () => {
  assert.equal(paneToggle({ showing: true, seen: 0 }), "hide");
  assert.equal(paneToggle({ showing: true, seen: 3 }), "hide");
  assert.equal(paneToggle({ showing: false, seen: 2 }), "show", "tabs to look at: the pane, nothing opened");
  assert.equal(paneToggle({ showing: false, seen: 0 }), "open", "none in sight: the door opens one — an act, where the pane by itself opens nothing");
  assert.equal(paneToggle({ showing: false, seen: undefined }), "open");
});

test("a tab beside the screen is at home in the conversation on screen, else the IDE's root, else the workflow or screen the route shows, else the workspace", () => {
  assert.deepEqual(screenHome({ kind: "channel", id: "c1" }, { scope: "workstream", id: "w1" }, { name: "workbench", id: "w1" }), { scope: "channel", id: "c1" }, "the conversation on screen first — it is what the person is reading");
  assert.deepEqual(screenHome({ kind: "goal", id: "g1" }, null, { name: "goal", id: "g1" }), { scope: "goal", id: "g1" });
  assert.deepEqual(screenHome({ kind: "dm", id: "d1" }, null, null), { scope: "dm", id: "d1" });
  assert.deepEqual(screenHome({ kind: "conversation", id: "k1" }, null, null), { scope: "conversation", id: "k1" });
  assert.deepEqual(screenHome(null, { scope: "workstream", id: "w1" }, { name: "workbench" }), { scope: "workstream", id: "w1" }, "the IDE's root");
  assert.deepEqual(screenHome(null, { scope: "goal", id: "g1" }, null), { scope: "goal", id: "g1" });
  assert.equal(screenHome(null, { scope: "machine", id: "m" }, null), null, "the machine is no home for a tab");
  assert.deepEqual(screenHome(null, null, { name: "workflow", id: "f1" }), { scope: "workflow", id: "f1" }, "the designer on a workflow");
  assert.deepEqual(screenHome(null, null, { name: "channel", id: "c2" }), { scope: "channel", id: "c2" }, "a channel screen whose conversation has not published yet");
  assert.equal(screenHome(null, null, { name: "pulse" }), null, "the workspace's home");
  assert.equal(screenHome(null, null, null), null);
});

test("a mode switch carries a browser tab at home here between the centre and the pane", () => {
  assert.deepEqual(followCentre("conversation", "b1", null), { show: "pane", key: "b1" }, "leaving documents: the strip's tab goes to the pane");
  assert.deepEqual(followCentre("board", "b1", null), { show: "pane", key: "b1" }, "the Board the same");
  assert.equal(followCentre("board", null, null), null, "no tab in the strip: nothing to carry");
  assert.deepEqual(followCentre("documents", null, "b2"), { show: "centre", key: "b2" }, "returning: the pane's tab at home here comes back to the strip");
  assert.equal(followCentre("documents", "b1", null), null, "returning with the pane on something else: nothing moves");
  assert.equal(followCentre("conversation", null, "b3"), null, "leaving with only the pane's tab: it is already where it goes");
});

/**
 * The *New drawing* dialog's rules. Run with
 * `node --test desktop/src/draw/newDrawingModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { canCreate, defaultTitle, firstTarget, targetKey, titleFollowsTemplate } from "./newDrawingModel.mjs";

test("a template proposes its label as the title; Empty proposes the untitled word", () => {
  assert.equal(defaultTitle({ id: "flowchart", label: "Flowchart" }, "Untitled drawing"), "Flowchart");
  assert.equal(defaultTitle({ id: "empty", label: "Empty" }, "Untitled drawing"), "Untitled drawing");
  assert.equal(defaultTitle(null, "Untitled drawing"), "Untitled drawing");
});

test("the title follows the template only while it still holds the last default", () => {
  assert.equal(titleFollowsTemplate("Flowchart", "Flowchart"), true);
  assert.equal(titleFollowsTemplate("  Flowchart ", "Flowchart"), true);
  assert.equal(titleFollowsTemplate("", "Flowchart"), true, "an emptied field follows too");
  assert.equal(titleFollowsTemplate("Orders in Q3", "Flowchart"), false, "a person's words are kept");
});

test("where: the place you stand on first, else the first offered, else none", () => {
  const ws = { scope: { scope: "workspace" }, label: "Workspace", here: false };
  const goal = { scope: { scope: "goal", id: "G1" }, label: "Launch", here: true };
  assert.equal(firstTarget([ws, goal]), goal);
  assert.equal(firstTarget([ws]), ws);
  assert.equal(firstTarget([]), null);
  assert.equal(targetKey(goal.scope), "goal:G1");
  assert.equal(targetKey(ws.scope), "workspace:");
});

test("Create takes a title with words in it", () => {
  assert.equal(canCreate("Orders"), true);
  assert.equal(canCreate("   "), false);
  assert.equal(canCreate(""), false);
});

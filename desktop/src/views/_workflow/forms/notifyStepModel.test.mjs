import test from "node:test";
import assert from "node:assert/strict";
import { scopeListed } from "./notifyStepModel.mjs";

test("a notify scope is listed when it is none, a channel the workspace has, or a declared text input", () => {
  const channels = ["general", "ops"];
  const inputs = ["channel", "desk"];
  for (const listed of [null, undefined, "", "general", "ops", "{inputs.channel}", "{inputs.desk}"]) assert.equal(scopeListed(listed, channels, inputs), true, String(listed));
  for (const unlisted of ["01ARZ3NDEKTSV4RRFFQ69G5FAV", "{inputs.gone}", "{steps.a.output.where}", "Ops"]) assert.equal(scopeListed(unlisted, channels, inputs), false, unlisted);
  assert.equal(scopeListed("general", [], []), false, "no channels listed: shown as it is");
  assert.equal(scopeListed("x", null, null), false);
});

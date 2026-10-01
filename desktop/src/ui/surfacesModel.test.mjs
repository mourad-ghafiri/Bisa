import { test } from "node:test";
import assert from "node:assert/strict";

import { layerMayShow, surfaces } from "./surfacesModel.mjs";

test("the open surfaces are counted with a floor, and a native layer shows only when none is open", () => {
  assert.equal(surfaces(0, 1), 1);
  assert.equal(surfaces(1, 1), 2);
  assert.equal(surfaces(2, -1), 1);
  assert.equal(surfaces(0, -1), 0, "a leave never goes below nought");
  assert.equal(surfaces(undefined, 1), 1);
  assert.ok(layerMayShow(0));
  assert.ok(!layerMayShow(1));
  assert.ok(layerMayShow(-3), "a count under nought reads as none");
});

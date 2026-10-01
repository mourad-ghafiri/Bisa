import { strict as assert } from "node:assert";
import { test } from "node:test";
import { olderThan } from "./pageCursorModel.mjs";

test("the node is asked for exactly what is older than the oldest row; the guest wire for the second after it", () => {
  const oldest = { id: "e1", created_at: 1700000000 };
  assert.deepEqual(olderThan(oldest, false), { at: 1700000000, id: "e1" });
  assert.deepEqual(olderThan(oldest, true), { at: 1700000001 });
});

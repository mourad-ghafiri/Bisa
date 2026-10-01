import test from "node:test";
import assert from "node:assert/strict";
import { BUDGET, RELEASE_AFTER_MS, acquire, emptyPool, holds, release, wants } from "./webglPoolModel.mjs";

test("a slot is granted while the budget lasts, held once, and refused past it", () => {
  let pool = emptyPool();
  for (let i = 0; i < BUDGET; i++) {
    const r = acquire(pool, `t${i}`);
    assert.ok(r.granted, `slot ${i}`);
    pool = r.pool;
  }
  const refused = acquire(pool, "one-too-many");
  assert.equal(refused.granted, false, "the DOM renderer draws the ninth");
  assert.equal(refused.pool, pool, "and nothing changed");
  const again = acquire(pool, "t3");
  assert.ok(again.granted && again.pool === pool, "a holder asking again keeps its slot");
});

test("a released slot is free for the next visible terminal; releasing a stranger changes nothing", () => {
  let pool = acquire(emptyPool(), "a").pool;
  pool = acquire(pool, "b").pool;
  assert.ok(holds(pool, "a"));
  const same = release(pool, "nobody");
  assert.equal(same, pool);
  pool = release(pool, "a");
  assert.ok(!holds(pool, "a") && holds(pool, "b"));
});

test("only a visible terminal wants the GPU, and a hidden one keeps it for two seconds", () => {
  assert.ok(wants({ visible: true }));
  assert.ok(!wants({ visible: false }));
  assert.ok(!wants({ visible: undefined }));
  assert.equal(RELEASE_AFTER_MS, 2000);
});

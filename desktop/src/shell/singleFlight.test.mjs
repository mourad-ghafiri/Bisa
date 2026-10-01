/**
 * The shared-fetch rule. Run with `node --test desktop/src/shell/singleFlight.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { createSingleFlight } from "./singleFlight.mjs";

/** A `start` whose outcome the test decides, recording every signal it was handed. */
function fakeStart() {
  const calls = [];
  const start = (signal) => {
    const call = { signal, resolve: null, reject: null };
    call.promise = new Promise((res, rej) => {
      call.resolve = res;
      call.reject = rej;
    });
    calls.push(call);
    return call.promise;
  };
  return { start, calls };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

test("two joiners share one fetch and both get its result", async () => {
  const sf = createSingleFlight();
  const { start, calls } = fakeStart();
  const a = sf.join("k", start);
  const b = sf.join("k", start);
  await tick();
  assert.equal(calls.length, 1, "one fetch for two askers");
  assert.equal(sf.joiners("k"), 2);
  calls[0].resolve("paths");
  assert.equal(await a, "paths");
  assert.equal(await b, "paths");
  assert.equal(sf.inFlight("k"), false, "a landed flight is forgotten");
});

test("the first joiner leaving does not abort the fetch; the last one leaving does", async () => {
  const sf = createSingleFlight();
  const { start, calls } = fakeStart();
  const first = new AbortController();
  const second = new AbortController();
  const a = sf.join("k", start, first.signal);
  const b = sf.join("k", start, second.signal);
  await tick();
  first.abort();
  await assert.rejects(a, { name: "AbortError" });
  assert.equal(calls[0].signal.aborted, false, "somebody is still waiting");
  assert.equal(sf.joiners("k"), 1);
  second.abort();
  await assert.rejects(b, { name: "AbortError" });
  assert.equal(calls[0].signal.aborted, true, "nobody is left: the fetch stops");
  assert.equal(sf.inFlight("k"), false);
});

test("strict mode: mount, unmount, mount again gets a fresh fetch rather than the aborted one", async () => {
  const sf = createSingleFlight();
  const { start, calls } = fakeStart();
  const mount1 = new AbortController();
  const p1 = sf.join("k", start, mount1.signal);
  await tick();
  mount1.abort();
  await assert.rejects(p1, { name: "AbortError" });
  const mount2 = new AbortController();
  const p2 = sf.join("k", start, mount2.signal);
  await tick();
  assert.equal(calls.length, 2, "the second mount starts its own flight");
  assert.equal(calls[1].signal.aborted, false);
  calls[1].resolve("fresh");
  assert.equal(await p2, "fresh");
});

test("a result lands for the joiner left even when the first asker is gone", async () => {
  const sf = createSingleFlight();
  const { start, calls } = fakeStart();
  const gone = new AbortController();
  const p1 = sf.join("k", start, gone.signal);
  const stays = sf.join("k", start);
  await tick();
  gone.abort();
  await assert.rejects(p1);
  calls[0].resolve("late");
  assert.equal(await stays, "late");
});

test("a failed flight is forgotten so the next call retries", async () => {
  const sf = createSingleFlight();
  const { start, calls } = fakeStart();
  const p = sf.join("k", start);
  await tick();
  calls[0].reject(new Error("boom"));
  await assert.rejects(p, /boom/);
  assert.equal(sf.inFlight("k"), false);
  const again = sf.join("k", start);
  await tick();
  assert.equal(calls.length, 2, "retried");
  calls[1].resolve("ok");
  assert.equal(await again, "ok");
});

test("a joiner whose signal is already aborted never starts anything", async () => {
  const sf = createSingleFlight();
  const { start, calls } = fakeStart();
  const dead = new AbortController();
  dead.abort();
  await assert.rejects(sf.join("k", start, dead.signal), { name: "AbortError" });
  assert.equal(calls.length, 0);
  assert.equal(sf.inFlight("k"), false);
});

test("different keys are different flights", async () => {
  const sf = createSingleFlight();
  const { start, calls } = fakeStart();
  const a = sf.join("a", start);
  const b = sf.join("b", start);
  await tick();
  assert.equal(calls.length, 2);
  calls[0].resolve(1);
  calls[1].resolve(2);
  assert.equal(await a, 1);
  assert.equal(await b, 2);
});

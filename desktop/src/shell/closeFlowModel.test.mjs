/**
 * The order of the way out: ask, save, go — and every way it stops.
 * Run with `node --test desktop/src/shell/closeFlowModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { CLOSE_OUTCOMES, closeFlow, declines, saveEvery } from "./closeFlowModel.mjs";

/** The window's hands, faked: what happened is in `said`, in the order it happened. */
function hands({ confirms = true, answer = true, dirty = true, saved = true, keeps = true } = {}) {
  const said = [];
  return {
    said,
    confirms: () => confirms,
    ask: async () => (said.push("ask"), answer),
    dirty: () => dirty,
    save: async () => (said.push("save"), saved),
    unsaved: () => said.push("unsaved"),
    keep: async () => {
      said.push("keep");
      if (!keeps) throw new Error("the storage refused");
    },
    keepFailed: (e) => said.push(`keep failed: ${e.message}`),
  };
}
const finish = (h) => async () => void h.said.push("finish");

test("the question comes before the save, and the save before the way out", async () => {
  const h = hands();
  assert.equal(await closeFlow(h).run(finish(h)), "closed");
  assert.deepEqual(h.said, ["ask", "save", "keep", "finish"]);
});

test("a no writes nothing and goes nowhere", async () => {
  const h = hands({ answer: false });
  assert.equal(await closeFlow(h).run(finish(h)), "cancelled");
  assert.deepEqual(h.said, ["ask"], "a cancelled quit has not saved a file");
});

test("the switch off asks nothing; nothing unsaved saves nothing", async () => {
  const quiet = hands({ confirms: false });
  assert.equal(await closeFlow(quiet).run(finish(quiet)), "closed");
  assert.deepEqual(quiet.said, ["save", "keep", "finish"]);
  const clean = hands({ confirms: false, dirty: false });
  assert.equal(await closeFlow(clean).run(finish(clean)), "closed");
  assert.deepEqual(clean.said, ["keep", "finish"]);
});

test("a save that fails keeps the window and says so", async () => {
  const h = hands({ saved: false });
  assert.equal(await closeFlow(h).run(finish(h)), "unsaved");
  assert.deepEqual(h.said, ["ask", "save", "unsaved"], "the window never went");
});

test("the red button, ⌘Q and the menu bar's Quit together are one flow: the second is dropped while the first is asked", async () => {
  let answerIt;
  const h = hands();
  h.ask = () => (h.said.push("ask"), new Promise((r) => (answerIt = r)));
  const flow = closeFlow(h);
  const first = flow.run(finish(h));
  assert.ok(flow.running());
  assert.equal(await flow.run(finish(h)), "busy");
  answerIt(true);
  assert.equal(await first, "closed");
  assert.deepEqual(h.said, ["ask", "save", "keep", "finish"], "asked once, saved once, kept once, gone once");
  assert.ok(!flow.running());
});

test("a flow that throws still ends: the next request is heard", async () => {
  const h = hands();
  const flow = closeFlow(h);
  await assert.rejects(flow.run(async () => Promise.reject(new Error("the shell refused"))), /the shell refused/);
  assert.ok(!flow.running(), "a failed way out does not hold the door shut for good");
  assert.equal(await flow.run(finish(h)), "closed");
  const thrower = hands();
  thrower.save = async () => Promise.reject(new Error("disk full"));
  const second = closeFlow(thrower);
  await assert.rejects(second.run(finish(thrower)), /disk full/);
  assert.ok(!second.running());
  assert.ok(!thrower.said.includes("finish"));
});

test("every outcome a flow answers is a named one", async () => {
  const seen = new Set();
  for (const opts of [{}, { answer: false }, { saved: false }]) {
    const h = hands(opts);
    seen.add(await closeFlow(h).run(finish(h)));
  }
  for (const o of seen) assert.ok(CLOSE_OUTCOMES.includes(o), o);
});

test("several dirty documents: each is tried, in order, and one failure is the whole answer", async () => {
  const tried = [];
  const save = (fails) => async (key) => (tried.push(key), !fails.includes(key));
  assert.equal(await saveEvery(["a", "b", "c"], save([])), true);
  assert.deepEqual(tried, ["a", "b", "c"]);
  tried.length = 0;
  assert.equal(await saveEvery(["a", "b", "c"], save(["a"])), false);
  assert.deepEqual(tried, ["a", "b", "c"], "the documents after the one that failed are still saved");
  assert.equal(await saveEvery([], save([])), true, "nothing to save is saved");
});

test("a save that throws is a save that failed, its reason heard, the rest still tried", async () => {
  const heard = [];
  const tried = [];
  const ok = await saveEvery(
    ["a", "b"],
    async (key) => {
      tried.push(key);
      if (key === "a") throw new Error("read-only file system");
      return true;
    },
    (key, e) => heard.push(`${key}: ${e.message}`),
  );
  assert.equal(ok, false);
  assert.deepEqual(tried, ["a", "b"]);
  assert.deepEqual(heard, ["a: read-only file system"]);
  assert.equal(await saveEvery(["a"], async () => "yes"), false, "only a true is a save");
});

test("what is remembered is kept after the save and before the way out, and never after a no or a failed save", async () => {
  const h = hands();
  assert.equal(await closeFlow(h).run(finish(h)), "closed");
  assert.deepEqual(h.said, ["ask", "save", "keep", "finish"]);
  const no = hands({ answer: false });
  await closeFlow(no).run(finish(no));
  assert.ok(!no.said.includes("keep"), "a cancelled quit keeps nothing: the window is still the person's");
  const unsaved = hands({ saved: false });
  await closeFlow(unsaved).run(finish(unsaved));
  assert.ok(!unsaved.said.includes("keep"));
});

test("a no and a document that would not save tell the shell no; a closed flow has answered, and a dropped second request leaves the answer to the first", () => {
  assert.equal(declines("cancelled"), true, "the OS asked and was told no");
  assert.equal(declines("unsaved"), true, "the window stays, so the quit is a no");
  assert.equal(declines("closed"), false, "quit_app answered");
  assert.equal(declines("busy"), false, "the question up answers for both; a no now would answer it early");
  for (const outcome of CLOSE_OUTCOMES) assert.equal(typeof declines(outcome), "boolean");
});

test("a keep that throws never holds the window", async () => {
  const h = hands({ keeps: false });
  assert.equal(await closeFlow(h).run(finish(h)), "closed");
  assert.deepEqual(h.said, ["ask", "save", "keep", "keep failed: the storage refused", "finish"]);
  // Nobody to hear why is no reason to stay either.
  const quiet = hands({ keeps: false });
  delete quiet.keepFailed;
  assert.equal(await closeFlow(quiet).run(finish(quiet)), "closed");
  assert.deepEqual(quiet.said, ["ask", "save", "keep", "finish"]);
});

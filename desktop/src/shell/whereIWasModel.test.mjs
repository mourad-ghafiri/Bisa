/**
 * *Forget where I was*: what is forgotten and what is kept, and the order —
 * save, forget, leave. Run with
 * `node --test desktop/src/shell/whereIWasModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { WHERE_I_WAS_OUTCOMES, FORGOTTEN, FORGOTTEN_ON_REQUEST, SEAL, forgetFlow, forgottenBy } from "./whereIWasModel.mjs";

/** The window's hands as fakes that write down what they were asked. */
function hands(over = {}) {
  const said = [];
  const forget = {};
  for (const what of [...FORGOTTEN, ...FORGOTTEN_ON_REQUEST]) forget[what] = () => void said.push(what);
  return {
    said,
    dirty: () => false,
    save: async () => (said.push("save"), true),
    forget,
    failed: (what, e) => void said.push(`failed:${what}:${e.message}`),
    leave: () => void said.push("leave"),
    ...over,
  };
}

test("every memory of where is forgotten, and the messages being written only when asked", () => {
  assert.deepEqual(forgottenBy(), ["memories", "root", "conversations"]);
  assert.deepEqual(forgottenBy({}), ["memories", "root", "conversations"]);
  assert.deepEqual(forgottenBy({ drafts: false }), ["memories", "root", "conversations"]);
  assert.deepEqual(forgottenBy({ drafts: true }), ["memories", "root", "conversations", "drafts"]);
  // A word that is not a yes is a no: a message is never discarded by accident.
  for (const odd of [undefined, null, 1, "true", "yes", {}]) assert.ok(!forgottenBy({ drafts: odd }).includes("drafts"), String(odd));
  // What is answered is the caller's own list.
  forgottenBy().push("theme");
  assert.deepEqual(forgottenBy(), ["memories", "root", "conversations"]);
  assert.ok(Object.isFrozen(FORGOTTEN) && Object.isFrozen(FORGOTTEN_ON_REQUEST));
});

test("the furniture that is no place is never on the list, and neither is a document", () => {
  const every = forgottenBy({ drafts: true });
  for (const kept of ["theme", "sidebar", "sizes", "docks", "window", "documents", "terminals", "browser", "locale"]) assert.ok(!every.includes(kept), kept);
  assert.equal(new Set(every).size, every.length, "each thing once");
});

test("each thing is forgotten in order, then the window leaves", async () => {
  const h = hands();
  assert.equal(await forgetFlow(h).run(), "forgotten");
  assert.deepEqual(h.said, ["memories", "root", "conversations", "leave"], "nothing unsaved: no save");
  const asked = hands();
  assert.equal(await forgetFlow(asked).run({ drafts: true }), "forgotten");
  assert.deepEqual(asked.said, ["memories", "root", "conversations", "drafts", "leave"]);
});

test("the memories are sealed once everything is forgotten and before the window leaves, whatever stayed", async () => {
  const h = hands({ dirty: () => true, seal: () => void h.said.push("seal") });
  assert.equal(await forgetFlow(h).run({ drafts: true }), "forgotten");
  assert.deepEqual(h.said, ["save", "memories", "root", "conversations", "drafts", "seal", "leave"]);
  // One thing that stayed does not leave the memories open.
  const stayed = hands({ seal: () => void stayed.said.push("seal") });
  stayed.forget.memories = () => {
    throw new Error("denied");
  };
  assert.equal(await forgetFlow(stayed).run(), "forgotten");
  assert.deepEqual(stayed.said, ["failed:memories:denied", "root", "conversations", "seal", "leave"]);
  // A seal that throws is heard by its name, and the window still leaves.
  const thrown = hands({
    seal: () => {
      throw new Error("no storage");
    },
  });
  assert.equal(await forgetFlow(thrown).run(), "forgotten");
  assert.deepEqual(thrown.said, ["memories", "root", "conversations", `failed:${SEAL}:no storage`, "leave"]);
  assert.ok(![...FORGOTTEN, ...FORGOTTEN_ON_REQUEST].includes(SEAL), "a seal is no thing forgotten");
  // Nothing is sealed when nothing was forgotten: the window stays, and goes on remembering.
  const unsaved = hands({ dirty: () => true, save: async () => false, seal: () => void unsaved.said.push("seal") });
  assert.equal(await forgetFlow(unsaved).run(), "unsaved");
  assert.deepEqual(unsaved.said, []);
});

test("what is unsaved is saved before anything is forgotten", async () => {
  const h = hands({ dirty: () => true });
  assert.equal(await forgetFlow(h).run(), "forgotten");
  assert.deepEqual(h.said, ["save", "memories", "root", "conversations", "leave"]);
});

test("a save that fails forgets nothing and the window stays", async () => {
  const h = hands({ dirty: () => true, save: async () => false });
  assert.equal(await forgetFlow(h).run({ drafts: true }), "unsaved");
  assert.deepEqual(h.said, []);
  // A save that throws is the caller's to hear, and still nothing is forgotten.
  const thrown = hands({
    dirty: () => true,
    save: async () => {
      throw new Error("disk full");
    },
  });
  await assert.rejects(forgetFlow(thrown).run(), /disk full/);
  assert.deepEqual(thrown.said, []);
});

test("one thing that stays does not keep the rest, and the window still leaves", async () => {
  const h = hands();
  h.forget.root = () => {
    throw new Error("denied");
  };
  assert.equal(await forgetFlow(h).run({ drafts: true }), "forgotten");
  assert.deepEqual(h.said, ["memories", "failed:root:denied", "conversations", "drafts", "leave"]);
  // With nobody to hear it, the same.
  const quiet = hands({ failed: undefined });
  quiet.forget.memories = () => {
    throw new Error("denied");
  };
  assert.equal(await forgetFlow(quiet).run(), "forgotten");
  assert.deepEqual(quiet.said, ["root", "conversations", "leave"]);
});

test("a second request while one is running is dropped, and the flow can run again after", async () => {
  let release;
  const held = new Promise((done) => {
    release = done;
  });
  const h = hands({ dirty: () => true, save: async () => (await held, true) });
  const flow = forgetFlow(h);
  const first = flow.run();
  assert.equal(flow.running(), true);
  assert.equal(await flow.run({ drafts: true }), "busy");
  release();
  assert.equal(await first, "forgotten");
  assert.deepEqual(h.said, ["memories", "root", "conversations", "leave"], "the dropped request forgot nothing of its own");
  assert.equal(flow.running(), false);
  // After a save that failed, too.
  const failing = forgetFlow(hands({ dirty: () => true, save: async () => false }));
  assert.equal(await failing.run(), "unsaved");
  assert.equal(failing.running(), false);
  assert.equal(await failing.run(), "unsaved");
});

test("every way the flow ends has its word", async () => {
  assert.deepEqual([...WHERE_I_WAS_OUTCOMES], ["busy", "unsaved", "forgotten"]);
  const seen = new Set();
  seen.add(await forgetFlow(hands()).run());
  seen.add(await forgetFlow(hands({ dirty: () => true, save: async () => false })).run());
  let release;
  const held = new Promise((done) => {
    release = done;
  });
  const flow = forgetFlow(hands({ leave: () => held }));
  const first = flow.run();
  seen.add(await flow.run());
  release();
  await first;
  assert.deepEqual([...seen].sort(), [...WHERE_I_WAS_OUTCOMES].sort());
});

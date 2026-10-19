/**
 * One model per open tab, kept across unmounts and let go with the tab. Run
 * with `node --test desktop/src/ui/monacoModelsModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { KeptModels } from "./monacoModelsModel.mjs";

const fake = (log, name) => ({ name, dispose: () => log.push(name) });

test("a tab's model is made once and answered again on every later mount — an unmount lets nothing go", () => {
  const kept = new KeptModels();
  const log = [];
  let made = 0;
  const make = () => (made++, fake(log, "a"));
  const first = kept.acquire("root|file:a.ts", make);
  const again = kept.acquire("root|file:a.ts", make);
  assert.equal(first, again, "the same model after the editor unmounted and mounted again");
  assert.equal(made, 1);
  assert.ok(kept.holds("root|file:a.ts"));
  assert.equal(kept.count, 1, "tabs holding a model, never mounts");
  assert.deepEqual(log, [], "an unmount is not a release");
});

test("a closed tab's model is disposed once and let go; the next open makes anew; a key nobody held is nothing", () => {
  const kept = new KeptModels();
  const log = [];
  kept.acquire("k", () => fake(log, "one"));
  assert.equal(kept.release(["k", "never-held"]), 1, "one went, the unknown key was nothing");
  assert.deepEqual(log, ["one"]);
  assert.ok(!kept.holds("k"));
  assert.equal(kept.count, 0);
  assert.equal(kept.release(["k"]), 0, "released once");
  assert.deepEqual(log, ["one"], "disposed once");
  const next = kept.acquire("k", () => fake(log, "two"));
  assert.equal(next.name, "two", "a fresh model for the reopened tab");
});

test("one document open in two roots is two tabs, so two models", () => {
  const kept = new KeptModels();
  const log = [];
  const a = kept.acquire("root-a|file:x.md", () => fake(log, "a"));
  const b = kept.acquire("root-b|file:x.md", () => fake(log, "b"));
  assert.notEqual(a, b);
  assert.equal(kept.count, 2);
  assert.equal(kept.release(["root-a|file:x.md"]), 1);
  assert.ok(kept.holds("root-b|file:x.md"), "the other root's tab keeps its model");
});

/**
 * The merge, rebase, pick and revert dialogs' words. Run with
 * `node --test desktop/src/views/_work/mergeModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { previewWords, MERGE_MODES, MERGE_MODE_WORDS, mergeConsent, mergeDefault, mergeMessagePlaceholder, mergeTakesMessage, pickConsent, pickOrder, rebaseConsent } from "./mergeModel.mjs";

test("the four merge modes each have a label and a sentence, the project's strategy picks the first one, and only a commit-making mode takes a message", () => {
  assert.deepEqual([...MERGE_MODES], ["ff", "no_ff", "ff_only", "squash"]);
  for (const m of MERGE_MODES) assert.ok(MERGE_MODE_WORDS[m].label && MERGE_MODE_WORDS[m].meaning.endsWith("."), m);
  assert.equal(mergeDefault("merge"), "no_ff");
  assert.equal(mergeDefault("squash"), "squash");
  assert.equal(mergeDefault("rebase"), "ff");
  assert.equal(mergeDefault(undefined), "ff");
  assert.ok(mergeTakesMessage("no_ff") && mergeTakesMessage("ff") && !mergeTakesMessage("ff_only") && !mergeTakesMessage("squash"));
  assert.equal(mergeMessagePlaceholder("feature", "main"), "Merge branch 'feature' into main");
});

test("every confirmation is a question with the verb as its button, short, and points at the card above for a conflict", () => {
  const m = mergeConsent("feature", "main", "squash");
  assert.equal(m.title, "Merge feature into main?");
  assert.match(m.body, /staged as one/);
  assert.match(m.body, /Resolve card above/);
  assert.equal(m.confirm, "Merge");
  const r = rebaseConsent("topic", "main", { autostash: true });
  assert.equal(r.title, "Rebase topic onto main?");
  assert.match(r.body, /stashed first and brought back/);
  const onto = rebaseConsent("topic", "main", { onto: "release" });
  assert.equal(onto.title, "Rebase topic onto release?");
  assert.match(onto.body, /since main are replayed on top of release/);
  const p = pickConsent(3, { recordOrigin: true });
  assert.equal(p.title, "Cherry-pick 3 commits?");
  assert.match(p.body, /oldest first/);
  assert.match(p.body, /naming where it came from/);
  assert.equal(pickConsent(1, { noCommit: true }).body.includes("as one staged change"), true);
  assert.equal(pickConsent(1).title, "Cherry-pick this commit?");
  for (const c of [m, r, onto, p]) {
    assert.ok(c.title.endsWith("?"));
    assert.ok(!c.danger);
    assert.ok(!c.body.includes("refs/bisa/safety/"), "the promise is SafetyNote's");
  }
});

test("a pick is sent oldest first, from a list drawn newest first", () => {
  const listed = [{ id: "c" }, { id: "b" }, { id: "a" }];
  assert.deepEqual(pickOrder(["a", "c"], listed), ["a", "c"]);
  assert.deepEqual(pickOrder(["b"], listed), ["b"]);
  assert.deepEqual(pickOrder(["zzz"], listed), []);
});

test("the look-ahead says nothing yet, clean, the files that would conflict, or that this git has none — a rebase as a likelihood", () => {
  assert.deepEqual(previewWords(null), { text: "Looking ahead for conflicts…", tone: "dim" });
  assert.equal(previewWords({ supported: false, clean: false, paths: [] }).tone, "dim");
  assert.deepEqual(previewWords({ supported: true, clean: true, paths: [] }), { text: "No conflicts expected.", tone: "ok" });
  assert.match(previewWords({ supported: true, clean: true, paths: [] }, { rebase: true }).text, /between the two tips — a commit along the way may still stop/);
  const two = previewWords({ supported: true, clean: false, paths: ["a.rs", "b.rs"] });
  assert.equal(two.text, "2 files would conflict: a.rs, b.rs. The Resolve card walks you through them.");
  assert.equal(two.tone, "warn");
  assert.equal(previewWords({ supported: true, clean: false, paths: ["a"] }).text, "1 file would conflict: a. The Resolve card walks you through it.");
  assert.match(previewWords({ supported: true, clean: false, paths: ["a", "b", "c", "d", "e", "f"] }).text, /a, b, c, d, … 2 more/);
});

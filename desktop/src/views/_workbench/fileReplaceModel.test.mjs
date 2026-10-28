import { strict as assert } from "node:assert";
import { test } from "node:test";
import { captureHint, previewRows, previewStale, previewWords, questionKey, replaceBody, replaceOutcomeWords, replacePlan } from "./fileReplaceModel.mjs";

const query = { needle: "needle", include: ["src/**"], exclude: ["*.lock"] };

test("the body is the box's question: a preview names no file, an apply names the plan", () => {
  const preview = replaceBody(query, { regex: true, caseSensitive: true, word: false }, "pin");
  assert.deepEqual(preview, { q: "needle", regex: true, case: "sensitive", word: false, include: ["src/**"], exclude: ["*.lock"], replacement: "pin", apply: false, files: [] });
  const apply = replaceBody(query, {}, "", [{ path: "a.rs", base_hash: "h1", extra: "dropped" }]);
  assert.equal(apply.apply, true);
  assert.equal(apply.case, "smart", "no switch is smart case, as the search sends it");
  assert.equal(apply.regex, false);
  assert.equal(apply.replacement, "", "an empty replacement deletes the match — sent as it is");
  assert.deepEqual(apply.files, [{ path: "a.rs", base_hash: "h1" }], "only the path and the hash travel: the body refuses unknown keys");
  assert.notEqual(preview.include, query.include, "the globs are copied, not shared");
});

test("a question's key moves with every knob, and a preview made for another is stale", () => {
  const key = questionKey(query, { regex: false }, "pin");
  assert.equal(key, questionKey({ ...query, include: ["src/**"] }, { regex: false, caseSensitive: false, word: false }, "pin"), "the same question, however the switches are left unsaid");
  for (const other of [
    questionKey({ ...query, needle: "needl" }, { regex: false }, "pin"),
    questionKey({ ...query, include: [] }, { regex: false }, "pin"),
    questionKey({ ...query, exclude: [] }, { regex: false }, "pin"),
    questionKey(query, { regex: true }, "pin"),
    questionKey(query, { caseSensitive: true }, "pin"),
    questionKey(query, { word: true }, "pin"),
    questionKey(query, { regex: false }, "pins"),
  ]) assert.notEqual(other, key);
  assert.equal(previewStale({ key }, key), false);
  assert.equal(previewStale({ key: "other" }, key), true);
  assert.equal(previewStale(null, key), false, "no preview is nothing to be stale");
  assert.equal(previewStale(undefined, key), false);
});

test("a preview folds into a file row, its lines, and a +N more row past the cap", () => {
  const files = [
    { path: "src/a.rs", base_hash: "ha", changes: [{ line: 3, before: "needle", after: "pin" }] },
    { path: "src/b.rs", base_hash: "hb", changes: [1, 2, 3].map((line) => ({ line, before: `n${line}`, after: `p${line}` })) },
  ];
  const rows = previewRows(files, 2);
  assert.deepEqual(
    rows.map((r) => r.key),
    ["f:src/a.rs", "c:src/a.rs:3", "f:src/b.rs", "c:src/b.rs:1", "c:src/b.rs:2", "m:src/b.rs"],
  );
  assert.deepEqual(rows[0], { kind: "file", key: "f:src/a.rs", path: "src/a.rs", count: 1, baseHash: "ha" });
  assert.deepEqual(rows[1], { kind: "change", key: "c:src/a.rs:3", path: "src/a.rs", line: 3, before: "needle", after: "pin" });
  assert.deepEqual(rows[5], { kind: "more", key: "m:src/b.rs", path: "src/b.rs", more: 1 });
  assert.equal(rows[2].count, 3, "the file row counts every line, folded or shown");
  assert.equal(previewRows(files).length, 6, "the default cap is the search's twenty — nothing folds here");
  assert.deepEqual(previewRows([]), []);
});

test("the plan names every previewed file or the one asked for, with its hash — and nothing is null", () => {
  const files = [
    { path: "a.rs", base_hash: "ha", changes: [] },
    { path: "b.rs", base_hash: "hb", changes: [] },
  ];
  assert.deepEqual(replacePlan(files), [
    { path: "a.rs", base_hash: "ha" },
    { path: "b.rs", base_hash: "hb" },
  ]);
  assert.deepEqual(replacePlan(files, "b.rs"), [{ path: "b.rs", base_hash: "hb" }]);
  assert.equal(replacePlan(files, "c.rs"), null, "a file the preview did not name is nothing to write");
  assert.equal(replacePlan([]), null, "the node refuses an apply that names no file");
});

test("the preview's words count lines and files, or say nothing would change", () => {
  assert.equal(previewWords([]), "Nothing would change — no writable file holds a match");
  assert.equal(previewWords([{ changes: [1] }]), "1 line in 1 file would change");
  assert.equal(previewWords([{ changes: [1, 2] }, { changes: [3] }]), "3 lines in 2 files would change");
});

test("the outcome line counts what went, what moved on and what failed, in the tone of the worst", () => {
  const ok = replaceOutcomeWords([
    { outcome: "applied", path: "a", changes: 1, hash: "x" },
    { outcome: "applied", path: "b", changes: 0, hash: "y" },
  ]);
  assert.deepEqual(ok, { tone: "ok", text: "2 files changed", applied: 2, skipped: 0, failed: 0 });
  const warn = replaceOutcomeWords([
    { outcome: "applied", path: "a", changes: 1, hash: "x" },
    { outcome: "skipped_changed", path: "b", current_hash: "z" },
  ]);
  assert.equal(warn.tone, "warn");
  assert.equal(warn.text, "1 file changed · 1 skipped — changed since the preview");
  const danger = replaceOutcomeWords([
    { outcome: "skipped_changed", path: "b", current_hash: "z" },
    { outcome: "failed", path: "../c", error: "outside the root" },
  ]);
  assert.equal(danger.tone, "danger");
  assert.equal(danger.text, "0 files changed · 1 skipped — changed since the preview · 1 failed");
  assert.deepEqual(replaceOutcomeWords([]), { tone: "ok", text: "0 files changed", applied: 0, skipped: 0, failed: 0 });
});

test("the hint says what a dollar means in the mode, and that empty deletes", () => {
  assert.equal(captureHint(false), "The text as typed — a $ is a dollar sign; empty deletes the match");
  assert.equal(captureHint(true), "$1 and $name name a group, $$ is a dollar sign; empty deletes the match");
});

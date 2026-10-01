/**
 * The explorer's clipboard as facts. Run with `node --test desktop/src/ui/fileClipboard.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { afterPaste, clipFrom, isCut, pasteSummary, pasteTargets } from "./fileClipboard.mjs";

const ROOT = "workstream:w1";

test("a clipboard holds distinct, non-empty paths under one root — or nothing", () => {
  assert.deepEqual(clipFrom("copy", ROOT, ["a.txt", "a.txt", "src"]), { kind: "copy", root: ROOT, paths: ["a.txt", "src"] });
  assert.equal(clipFrom("cut", ROOT, []), null);
  assert.equal(clipFrom("cut", ROOT, [""]), null, "the root itself is never on the clipboard");
  assert.ok(isCut(clipFrom("cut", ROOT, ["a.txt"]), "a.txt"));
  assert.ok(!isCut(clipFrom("copy", ROOT, ["a.txt"]), "a.txt"), "a copied row is not dimmed");
  assert.ok(!isCut(null, "a.txt"));
});

test("a copy pastes as a copy, renamed the way a duplicate is when the name is taken", () => {
  const clip = clipFrom("copy", ROOT, ["src/a.txt", "src/b.txt"]);
  const r = pasteTargets(clip, { targetDir: "docs", root: ROOT, existing: ["a.txt"] });
  assert.deepEqual(r, {
    ops: [
      { from: "src/a.txt", to: "docs/a copy.txt", op: "copy" },
      { from: "src/b.txt", to: "docs/b.txt", op: "copy" },
    ],
  });
  const same = pasteTargets(clip, { targetDir: "src", root: ROOT, existing: ["a.txt", "b.txt"] });
  assert.deepEqual(same.ops.map((o) => o.to), ["src/a copy.txt", "src/b copy.txt"], "beside itself is a duplicate");
  assert.deepEqual(pasteTargets(clip, { targetDir: "", root: ROOT, existing: [] }).ops.map((o) => o.to), ["a.txt", "b.txt"], "into the root");
  assert.equal(afterPaste(clip), clip, "a copy can be pasted again");
});

test("a cut pastes as a move, never into itself, never over a sibling, and is spent by the paste", () => {
  const clip = clipFrom("cut", ROOT, ["src"]);
  assert.deepEqual(pasteTargets(clip, { targetDir: "lib", root: ROOT, existing: [] }), { ops: [{ from: "src", to: "lib/src", op: "move" }] });
  assert.deepEqual(pasteTargets(clip, { targetDir: "src/inner", root: ROOT, existing: [] }), { refused: "src cannot be moved into itself." });
  assert.deepEqual(pasteTargets(clip, { targetDir: "src", root: ROOT, existing: [] }), { refused: "src cannot be moved into itself." });
  assert.deepEqual(pasteTargets(clip, { targetDir: "", root: ROOT, existing: ["src"] }), { refused: "Already there.", idle: true }, "where it already sits is nothing to do — and says so by a flag, never by its words");
  assert.deepEqual(pasteTargets(clip, { targetDir: "lib", root: ROOT, existing: ["src"] }), { refused: "src is already in lib." });
  assert.equal(afterPaste(clip), null);
});

test("a paste into another tree is refused, and an empty clipboard says so", () => {
  const clip = clipFrom("copy", ROOT, ["a.txt"]);
  assert.match(pasteTargets(clip, { targetDir: "", root: "workstream:w2", existing: [] }).refused, /another tree/);
  assert.deepEqual(pasteTargets(null, { targetDir: "", root: ROOT, existing: [] }), { refused: "Nothing to paste." });
  assert.equal(pasteSummary([{ op: "move" }, { op: "copy" }, { op: "copy" }]), "1 moved, 2 copied.");
  assert.equal(pasteSummary([{ op: "copy" }]), "1 copied.");
});

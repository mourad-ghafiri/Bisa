/**
 * A new file's patch is git's own form, and the hunk parser reads it back
 * whole. Run with `node --test desktop/src/views/_work/newFilePatch.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { hunkPatch, parseHunks } from "./hunkModel.mjs";
import { newFilePatch, newFileWords } from "./newFilePatch.mjs";

const HEADER = ["diff --git a/src/a.rs b/src/a.rs", "new file mode 100644", "--- /dev/null", "+++ b/src/a.rs"];

test("a new file's patch is git's own form", () => {
  assert.equal(newFilePatch("src/a.rs", "one\ntwo\n"), [...HEADER, "@@ -0,0 +1,2 @@", "+one", "+two"].join("\n") + "\n");
  assert.equal(newFilePatch("src/a.rs", "only\n"), [...HEADER, "@@ -0,0 +1 @@", "+only"].join("\n") + "\n", "a count of one is left off, as git writes it");
  assert.equal(
    newFilePatch("src/a.rs", "one\nlast"),
    [...HEADER, "@@ -0,0 +1,2 @@", "+one", "+last", "\\ No newline at end of file"].join("\n") + "\n",
    "the marker follows the last line when the file does not end in a newline",
  );
  assert.equal(newFilePatch("src/a.rs", ""), HEADER.join("\n") + "\n", "an empty file has no hunk");
  assert.match(newFilePatch("docs/my notes.md", "x\n"), /^diff --git a\/docs\/my notes\.md b\/docs\/my notes\.md\n/, "a path with a space is kept verbatim");
  assert.ok(newFilePatch("a.txt", "one\r\ntwo\r\n").includes("+one\r\n+two\r\n"), "a CR stays on its line, as git keeps it");
});

test("the parser reads it back whole, and a hunk round-trips", () => {
  const diff = newFilePatch("src/a.rs", "one\ntwo\nthree");
  const parsed = parseHunks(diff);
  assert.equal(parsed.header, HEADER.join("\n") + "\n");
  assert.equal(parsed.hunks.length, 1);
  const [hunk] = parsed.hunks;
  assert.equal(hunk.oldCount, 0);
  assert.equal(hunk.newCount, 3);
  assert.deepEqual(
    hunk.lines.map((l) => [l.kind, l.oldLine, l.newLine]),
    [
      ["add", null, 1],
      ["add", null, 2],
      ["add", null, 3],
      ["meta", null, null],
    ],
    "every line an addition, numbered from one, the marker travelling with the last",
  );
  assert.equal(hunkPatch(parsed.header, hunk), diff, "the hunk alone is the whole patch again");
  assert.equal(parseHunks(newFilePatch("src/a.rs", "")).hunks.length, 0);
});

test("the words say what a new file is", () => {
  const text = newFileWords({ binary: false, size: 120, truncated: false });
  assert.match(text.sentence, /^New file — git has never seen it; every line is an addition, and it is staged whole from the list\.$/);
  assert.equal(text.binary, false);
  assert.equal(text.cut, false);
  const binary = newFileWords({ binary: true, size: 2048, truncated: false });
  assert.match(binary.sentence, /^New binary file — 2\.0 KB; git has never seen it, and it arrives whole in the commit\.$/);
  assert.equal(binary.binary, true);
  assert.equal(newFileWords({ binary: false, size: 3_000_000, truncated: true }).cut, true, "a file cut at the read cap says so");
  assert.equal(newFileWords(null).binary, false);
});

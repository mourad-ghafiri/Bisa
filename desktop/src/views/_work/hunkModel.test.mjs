import { test } from "node:test"; import assert from "node:assert/strict"; import {   hunkPatch, hunkRange, linesPatch, noteScope, parseHunks, pickable, scopeLabel, unsentNotes, hunkPosition, reviewNoteGone } from "./hunkModel.mjs";

const DIFF = [
  "diff --git a/big.txt b/big.txt",
  "index 1111111..2222222 100644",
  "--- a/big.txt",
  "+++ b/big.txt",
  "@@ -1,4 +1,4 @@",
  " line 1",
  "-line 2",
  "+line 2 edited",
  " line 3",
  " line 4",
  "@@ -26,5 +26,6 @@ fn tail() {",
  " line 26",
  " line 27",
  "-line 28",
  "+line 28 edited",
  "+line 28 and a half",
  " line 29",
  " line 30",
  "\\ No newline at end of file",
  "",
].join("\n");

test("a diff splits into its header and its hunks with line numbers on both sides", () => {
  const { header, hunks } = parseHunks(DIFF);
  assert.equal(header, "diff --git a/big.txt b/big.txt\nindex 1111111..2222222 100644\n--- a/big.txt\n+++ b/big.txt\n");
  assert.equal(hunks.length, 2);
  const [a, b] = hunks;
  assert.deepEqual([a.oldStart, a.oldCount, a.newStart, a.newCount], [1, 4, 1, 4]);
  assert.equal(a.context, "");
  assert.equal(b.context, "fn tail() {");
  assert.deepEqual(
    a.lines.map((l) => [l.kind, l.oldLine, l.newLine]),
    [
      ["context", 1, 1],
      ["del", 2, null],
      ["add", null, 2],
      ["context", 3, 3],
      ["context", 4, 4],
    ],
  );
  // The marker rides along and numbers nothing.
  assert.equal(b.lines.at(-1).kind, "meta");
  assert.equal(b.lines[3].kind, "add");
  assert.equal(b.lines[3].newLine, 28);
  assert.equal(b.lines[4].newLine, 29);
  // The hunk's own text round-trips: header + the whole hunk = a valid patch.
  assert.equal(hunkPatch(header, a), header + "@@ -1,4 +1,4 @@\n line 1\n-line 2\n+line 2 edited\n line 3\n line 4\n");
});

test("a hunk with a count of one omits the count, as git does", () => {
  const { hunks } = parseHunks("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n");
  assert.deepEqual([hunks[0].oldCount, hunks[0].newCount], [1, 1]);
  assert.equal(linesPatch("--- a/x\n+++ b/x\n", hunks[0], [0, 1]), "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n");
});

test("a patch of picked lines keeps unpicked deletions as context and drops unpicked additions", () => {
  const { header, hunks } = parseHunks(DIFF);
  const b = hunks[1];
  // Pick only "+line 28 and a half" (index 4). The -/+ pair for line 28 is
  // unpicked: the deletion becomes context, the addition disappears.
  const patch = linesPatch(header, b, [4]);
  assert.equal(
    patch,
    header +
      [
        "@@ -26,5 +26,6 @@ fn tail() {",
        " line 26",
        " line 27",
        " line 28",
        "+line 28 and a half",
        " line 29",
        " line 30",
        "\\ No newline at end of file",
        "",
      ].join("\n"),
  );
  // Nothing picked, or only context, is not a patch.
  assert.equal(linesPatch(header, b, []), null);
  assert.equal(linesPatch(header, b, [0, 1]), null);
  // Picking the deletion alone recounts: old side unchanged, new side one short.
  const delOnly = linesPatch(header, b, [2]);
  assert.match(delOnly, /@@ -26,5 \+26,4 @@ fn tail\(\) \{\n/);
  assert.ok(!delOnly.includes("+line 28"));
});

test("pickable names the change lines and hunkRange points at the new side", () => {
  const { hunks } = parseHunks(DIFF);
  assert.deepEqual(pickable(hunks[0]), [1, 2]);
  assert.deepEqual(pickable(hunks[1]), [2, 3, 4]);
  assert.deepEqual(hunkRange(hunks[0]), { start: 1, end: 4 });
  assert.deepEqual(hunkRange(hunks[1]), { start: 26, end: 31 });
  // A pure deletion hunk: "+0,0" style has no new lines; the range is one line, never 0.
  const gone = parseHunks("--- a/x\n+++ b/x\n@@ -1,2 +0,0 @@\n-a\n-b\n").hunks[0];
  assert.deepEqual(hunkRange(gone), { start: 1, end: 1 });
});

test("scopes and the unsent filter", () => {
  assert.deepEqual(noteScope(true), { scope: "staged" });
  assert.deepEqual(noteScope(false), { scope: "unstaged" });
  assert.equal(scopeLabel({ scope: "branch", base: "main" }), "vs main");
  assert.equal(scopeLabel({ scope: "staged" }), "staged");
  const notes = [
    { id: "a", sent_at: null, resolved_at: null },
    { id: "b", sent_at: 5, resolved_at: null },
    { id: "c", sent_at: null, resolved_at: 9 },
    { id: "d" },
  ];
  assert.deepEqual(unsentNotes(notes).map((n) => n.id), ["a", "d"]);
});

test("an empty or headerless diff parses to nothing rather than throwing", () => {
  assert.deepEqual(parseHunks(""), { header: "", hunks: [] });
  assert.deepEqual(parseHunks("just words\n").hunks, []);
});

test("a hunk's place is counted from one, in the catalog's words", () => {
  assert.equal(hunkPosition(0, 5), "hunk 1 of 5");
  assert.equal(hunkPosition(4, 5), "hunk 5 of 5");
});

test("a review note that is gone is a 404 on its edit, its resolve and its delete — the list reads again and the row leaves", async () => {
  assert.equal(reviewNoteGone(404), true);
  for (const other of [400, 409, 500, 0, null, undefined]) assert.equal(reviewNoteGone(other), false);
  const { readFileSync } = await import("node:fs");
  const list = readFileSync(new URL("./ReviewNotes.tsx", import.meta.url), "utf8");
  const act = list.slice(list.indexOf("const act = async"), list.indexOf("const send = () =>"));
  assert.ok(act.includes('toast.error(failureText("work", "review-notes-failed", e));'), "the node's own sentence is said — and an exception's text never is");
  assert.ok(act.includes("if (e instanceof ApiError && reviewNoteGone(e.status)) notes.reload();"), "and a note that is gone is read out of the list");
  assert.ok(!/status === 400|not found/i.test(list), "no old status, no words of a refusal");
  for (const verb of ["api.reviewNoteEdit(", "api.reviewNoteResolve(", "api.reviewNoteDelete("]) assert.ok(list.includes(`act("note", () => ${verb}`), `${verb} goes through the one door`);
});

test("a marker after an unpicked deletion stays — the deletion turned context still names a last line without a newline, and git applies the patch only with it — while a marker of an unpicked addition goes with it", () => {
  // git's own diff of "a\nx\nb\nlast" (no newline at the end) → "a\ny\nb\nlast2\n".
  const diff = ["diff --git a/f.txt b/f.txt", "--- a/f.txt", "+++ b/f.txt", "@@ -1,4 +1,4 @@", " a", "-x", "+y", " b", "-last", "\\ No newline at end of file", "+last2", ""].join("\n");
  const { header, hunks } = parseHunks(diff);
  const h = hunks[0];
  // Only the x→y pair: the tail's deletion becomes context and keeps its marker.
  assert.equal(linesPatch(header, h, [1, 2]), header + ["@@ -1,4 +1,4 @@", " a", "-x", "+y", " b", " last", "\\ No newline at end of file", ""].join("\n"));
  // Only the tail's deletion: the marker stays with it, the addition is dropped.
  assert.equal(linesPatch(header, h, [4]), header + ["@@ -1,4 +1,3 @@", " a", " x", " b", "-last", "\\ No newline at end of file", ""].join("\n"));
  // A marker of an unpicked addition goes with it.
  const grows = ["diff --git a/g.txt b/g.txt", "--- a/g.txt", "+++ b/g.txt", "@@ -1,2 +1,3 @@", " a", "-b", "+b", "+c", "\\ No newline at end of file", ""].join("\n");
  const g = parseHunks(grows);
  assert.equal(linesPatch(g.header, g.hunks[0], [1]), g.header + ["@@ -1,2 +1 @@", " a", "-b", ""].join("\n"));
  // The marker alone is no pick.
  assert.equal(linesPatch(header, h, [5]), null);
});

test("a diff with Windows line endings keeps each line's carriage return and still finds its hunks; a new file's hunk starts at the first line", () => {
  const crlf = ["diff --git a/w.txt b/w.txt", "--- a/w.txt", "+++ b/w.txt", "@@ -1,2 +1,2 @@ fn main() {\r", " a\r", "-b\r", "+c\r", ""].join("\n");
  const { header, hunks } = parseHunks(crlf);
  assert.equal(hunks.length, 1, "the header line is read past its carriage return");
  assert.equal(hunks[0].context, "fn main() {");
  assert.deepEqual(hunks[0].lines.map((l) => l.text), [" a\r", "-b\r", "+c\r"], "the lines keep the file's endings, so the patch round-trips");
  assert.equal(hunkPatch(header, hunks[0]), header + "@@ -1,2 +1,2 @@ fn main() {\r\n a\r\n-b\r\n+c\r\n");
  const fresh = parseHunks(["--- /dev/null", "+++ b/n.txt", "@@ -0,0 +1,3 @@", "+one", "+two", "+three", ""].join("\n"));
  assert.deepEqual([fresh.hunks[0].oldStart, fresh.hunks[0].oldCount, fresh.hunks[0].newStart, fresh.hunks[0].newCount], [0, 0, 1, 3]);
  assert.deepEqual(hunkRange(fresh.hunks[0]), { start: 1, end: 3 });
});

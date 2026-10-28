import { strict as assert } from "node:assert";
import { test } from "node:test";
import { problemCounts, problemRows, problemsBadge, problemsWords, severityOf, severityWord, withDiagnostics, withoutLanguage } from "./problemsModel.mjs";

const at = (line, character = 0) => ({ start: { line, character }, end: { line, character: character + 1 } });
const err = (line, message, extra = {}) => ({ range: at(line), message, severity: 1, ...extra });
const warn = (line, message) => ({ range: at(line, 4), message, severity: 2 });

test("LSP's severities read by number, an absent one as an error, each with its word", () => {
  assert.equal(severityOf(1), "error");
  assert.equal(severityOf(2), "warning");
  assert.equal(severityOf(3), "info");
  assert.equal(severityOf(4), "hint");
  assert.equal(severityOf(undefined), "error", "absent is an error, as the gutter's markers read it");
  assert.equal(severityOf(9), "error");
  assert.equal(severityWord("error"), "error");
  assert.equal(severityWord("warning"), "warning");
  assert.equal(severityWord("info"), "info");
  assert.equal(severityWord("hint"), "hint");
});

test("a frame replaces the pair's list whole, adds a new pair, removes an emptied one, and moves nothing heard twice", () => {
  const first = withDiagnostics([], { language: "rust", uri: "src/a.rs", diagnostics: [err(0, "x")] });
  assert.equal(first.length, 1);
  const two = withDiagnostics(first, { language: "rust", uri: "src/b.rs", diagnostics: [warn(2, "y")] });
  assert.deepEqual(
    two.map((e) => e.uri),
    ["src/a.rs", "src/b.rs"],
  );
  const replaced = withDiagnostics(two, { language: "rust", uri: "src/a.rs", diagnostics: [err(5, "z")] });
  assert.equal(replaced[0].diagnostics[0].message, "z", "the last list published is the list");
  assert.equal(replaced[1], two[1], "the other pair is untouched");
  const same = withDiagnostics(replaced, { language: "rust", uri: "src/a.rs", diagnostics: replaced[0].diagnostics });
  assert.equal(same, replaced, "the same list heard again is the same array");
  const emptied = withDiagnostics(replaced, { language: "rust", uri: "src/a.rs", diagnostics: [] });
  assert.deepEqual(
    emptied.map((e) => e.uri),
    ["src/b.rs"],
    "a document with nothing wrong earns no row",
  );
  assert.equal(withDiagnostics(emptied, { language: "rust", uri: "src/zz.rs", diagnostics: [] }), emptied, "an empty list for an unknown pair changes nothing");
  // Two servers on one path are two pairs.
  const twoServers = withDiagnostics(first, { language: "typescript", uri: "src/a.rs", diagnostics: [warn(1, "w")] });
  assert.equal(twoServers.length, 2);
});

test("a server that stopped takes its word with it, and no other's", () => {
  const entries = [
    { language: "rust", uri: "a.rs", diagnostics: [err(0, "x")] },
    { language: "typescript", uri: "a.ts", diagnostics: [err(0, "y")] },
  ];
  const left = withoutLanguage(entries, "rust");
  assert.deepEqual(
    left.map((e) => e.language),
    ["typescript"],
  );
  assert.equal(withoutLanguage(entries, "go"), entries, "nothing of that server's: the same array");
});

test("the counts are by severity and by file, over every server's word", () => {
  const entries = [
    { language: "rust", uri: "src/a.rs", diagnostics: [err(0, "x"), warn(1, "w"), { range: at(3), message: "i", severity: 3 }, { range: at(4), message: "h", severity: 4 }] },
    { language: "rust", uri: "src/b.rs", diagnostics: [err(0, "y")] },
    { language: "typescript", uri: "src/b.rs", diagnostics: [warn(0, "z")] },
    { language: "rust", uri: "src/c.rs", diagnostics: [] },
  ];
  assert.deepEqual(problemCounts(entries), { errors: 2, warnings: 2, infos: 1, hints: 1, files: 2 });
  assert.deepEqual(problemCounts([]), { errors: 0, warnings: 0, infos: 0, hints: 0, files: 0 });
});

test("the rows are a file row per path in path order, every server's word under it, errors first then by line, folded past the cap", () => {
  const entries = [
    { language: "rust", uri: "src/b.rs", diagnostics: [warn(9, "late warning"), err(7, "late error", { code: { value: "E0308" }, source: "rustc" }), err(2, "early error")] },
    { language: "typescript", uri: "src/b.rs", diagnostics: [{ range: at(0), message: "ts says", severity: 2, code: 2322 }] },
    { language: "rust", uri: "src/a.rs", diagnostics: [err(0, "a")] },
  ];
  const rows = problemRows(entries);
  assert.deepEqual(
    rows.map((r) => r.key),
    ["f:src/a.rs", "p:src/a.rs:0", "f:src/b.rs", "p:src/b.rs:0", "p:src/b.rs:1", "p:src/b.rs:2", "p:src/b.rs:3"],
  );
  assert.deepEqual(rows[0], { kind: "file", key: "f:src/a.rs", path: "src/a.rs", errors: 1, warnings: 0, count: 1 });
  assert.deepEqual(rows[1], { kind: "problem", key: "p:src/a.rs:0", path: "src/a.rs", line: 1, column: 1, severity: "error", message: "a", source: "rust", code: null });
  assert.deepEqual(
    rows.slice(3).map((r) => [r.severity, r.line, r.message]),
    [
      ["error", 3, "early error"],
      ["error", 8, "late error"],
      ["warning", 1, "ts says"],
      ["warning", 10, "late warning"],
    ],
    "errors before warnings, each by line; LSP's 0-based lines read 1-based",
  );
  assert.equal(rows[4].code, "E0308", "a coded diagnostic's code, an object's value too");
  assert.equal(rows[4].source, "rustc", "the server's own source when it names one");
  assert.equal(rows[5].code, "2322");
  assert.equal(rows[5].source, "typescript", "else the language");
  assert.equal(rows[2].errors, 2);
  assert.equal(rows[2].warnings, 2);
  const folded = problemRows(entries, 2);
  assert.deepEqual(folded.at(-1), { kind: "more", key: "m:src/b.rs", path: "src/b.rs", more: 2 });
  assert.deepEqual(problemRows([]), []);
  assert.deepEqual(problemRows([{ language: "rust", uri: "x.rs", diagnostics: [] }]), [], "a document with nothing wrong draws no row");
});

test("the rail's mark stands for an error alone, and the words count what stands", () => {
  const none = problemCounts([]);
  assert.equal(problemsBadge(none), null);
  assert.equal(problemsWords(none), "No problems reported");
  const warningsOnly = { errors: 0, warnings: 3, infos: 0, hints: 0, files: 1 };
  assert.equal(problemsBadge(warningsOnly), null, "a warning does not stop the work: no mark");
  assert.equal(problemsWords(warningsOnly), "0 errors · 3 warnings in 1 file");
  const some = { errors: 1, warnings: 1, infos: 2, hints: 0, files: 2 };
  assert.deepEqual(problemsBadge(some), { tone: "danger", title: "1 error · 1 warning in 2 files" });
  assert.equal(problemsWords({ errors: 0, warnings: 0, infos: 1, hints: 0, files: 1 }), "0 errors · 0 warnings in 1 file", "an info alone is still something reported");
});

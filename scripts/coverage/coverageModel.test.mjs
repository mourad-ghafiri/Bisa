/**
 * The coverage gate's own rules, and the two committed files held to the
 * tree: every excluded path exists, would otherwise be measured and says why;
 * every baseline key is a tree on disk with a percentage of at most one
 * decimal. Run with `node --test scripts/coverage/coverageModel.test.mjs`.
 * The lcov fixtures are template strings: no fixture file, nothing measured.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  BASELINE_FILE,
  TARGET,
  applyExclusions,
  bareRanges,
  codeBraces,
  completeDesktop,
  excludedLines,
  fileStats,
  formatReport,
  judge,
  mergeLcov,
  nextBaseline,
  parseLcov,
  percent,
  placeOf,
  relativize,
  testItemSpans,
  treeTotals,
  questionEdgeLines,
  withoutExcused,
} from "./coverageModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (rel) => readFileSync(join(root, rel), "utf8");

const ROOT = "/home/me/Bisa";

/** cargo-llvm-cov's lcov: functions, branches and totals around the lines. */
const LLVM = `SF:${ROOT}/crates/bisa-core/src/id.rs
FN:3,_ZN4bisa2id3new
FNDA:4,_ZN4bisa2id3new
FNF:1
FNH:1
DA:3,4
DA:4,4
DA:5,0
BRF:0
BRH:0
LF:3
LH:2
end_of_record
SF:${ROOT}/crates/bisa-core/tests/it/shapes.rs
DA:1,1
DA:2,1
LF:2
LH:2
end_of_record
SF:${ROOT}/crates/bisa-core/src/id.rs
DA:3,0
DA:5,2
DA:9,0
end_of_record
`;

/** Node's lcov: a test name line, functions, branches with a checksum-free DA. */
const NODE = `TN:
SF:${ROOT}/desktop/src/shell/trayModel.mjs
FN:1,tone
FNF:1
FNH:1
FNDA:3,tone
DA:1,3
DA:2,3
DA:3,0
BRDA:2,0,0,3
BRF:1
BRH:1
LF:3
LH:2
end_of_record
TN:
SF:${ROOT}/desktop/src/shell/trayModel.test.mjs
DA:1,1
LF:1
LH:1
end_of_record
`;

test("an lcov from llvm-cov and one from node parse to the same shape, and a file seen twice keeps the higher count per line", () => {
  const llvm = parseLcov(LLVM);
  const id = llvm.get(`${ROOT}/crates/bisa-core/src/id.rs`);
  assert.deepEqual([...id.entries()], [
    [3, 4],
    [4, 4],
    [5, 2],
    [9, 0],
  ]);
  assert.ok(llvm.has(`${ROOT}/crates/bisa-core/tests/it/shapes.rs`), "a test source is parsed too; placement drops it later");
  const node = parseLcov(NODE);
  assert.deepEqual([...node.get(`${ROOT}/desktop/src/shell/trayModel.mjs`).entries()], [
    [1, 3],
    [2, 3],
    [3, 0],
  ]);
  const merged = mergeLcov([llvm, node, parseLcov(`SF:${ROOT}/desktop/src/shell/trayModel.mjs\nDA:3,1\nend_of_record\n`)]);
  assert.equal(merged.get(`${ROOT}/desktop/src/shell/trayModel.mjs`).get(3), 1, "a later file's hit on a bare line counts");
  assert.equal(merged.get(`${ROOT}/crates/bisa-core/src/id.rs`).get(3), 4);
  assert.deepEqual([...parseLcov("DA:1,1\nend_of_record\n").keys()], [], "a DA before any SF is nobody's");
  assert.deepEqual([...parseLcov("SF:a.rs\nDA:x,1\nDA:2,\nend_of_record\n").get("a.rs").entries()], [], "a line that is not a number is read past");
});

test("a path is the repository's own under the root, kept when relative, and dropped when it is somebody else's", () => {
  assert.equal(relativize(`${ROOT}/crates/bisa-core/src/id.rs`, ROOT), "crates/bisa-core/src/id.rs");
  assert.equal(relativize(`${ROOT}/crates/bisa-core/src/id.rs`, `${ROOT}/`), "crates/bisa-core/src/id.rs", "a trailing slash on the root is nothing");
  assert.equal(relativize("./desktop/src/a.mjs", ROOT), "desktop/src/a.mjs");
  assert.equal(relativize("desktop/src/a.mjs", ROOT), "desktop/src/a.mjs");
  assert.equal(relativize("src/shell/trayModel.mjs", ROOT, "desktop"), "desktop/src/shell/trayModel.mjs", "Node writes paths from the folder it ran in");
  assert.equal(relativize("./src/a.mjs", ROOT, "desktop/"), "desktop/src/a.mjs");
  assert.equal(relativize(`${ROOT}/desktop/src/a.mjs`, ROOT, "desktop"), "desktop/src/a.mjs", "an absolute path ignores the base");
  assert.equal(relativize("/home/me/.cargo/registry/src/x/serde/lib.rs", ROOT), null, "a dependency's source");
  assert.equal(relativize("C:/other/lib.rs", ROOT), null);
});

test("a file is placed in its tree: a crate's src, the shell, a desktop directory cut to two segments; tests, benches, build scripts, declarations and .test.mjs are nobody's", () => {
  assert.equal(placeOf("crates/bisa-core/src/id.rs"), "crates/bisa-core");
  assert.equal(placeOf("crates/bisa-engine/src/ide/files.rs"), "crates/bisa-engine");
  assert.equal(placeOf("crates/bisa-node/src/bin/api-docs.rs"), "crates/bisa-node", "a binary is the crate's");
  assert.equal(placeOf("crates/bisa-core/tests/it/shapes.rs"), null);
  assert.equal(placeOf("crates/bisa-engine/benches/graph.rs"), null);
  assert.equal(placeOf("crates/bisa-store/build.rs"), null);
  assert.equal(placeOf("crates/bisa-cli/tests/scripted_agent.rs"), null);
  assert.equal(placeOf("desktop/src-tauri/src/main.rs"), "desktop/src-tauri");
  assert.equal(placeOf("desktop/src-tauri/src/tray/mod.rs"), "desktop/src-tauri");
  assert.equal(placeOf("desktop/src/apiModel.mjs"), "desktop/src");
  assert.equal(placeOf("desktop/src/shell/trayModel.mjs"), "desktop/src/shell");
  assert.equal(placeOf("desktop/src/views/_work/retireModel.mjs"), "desktop/src/views/_work");
  assert.equal(placeOf("desktop/src/views/_workflow/forms/startForm.mjs"), "desktop/src/views/_workflow", "cut to two segments");
  assert.equal(placeOf("desktop/src/ui/artifact/sheetModel.mjs"), "desktop/src/ui/artifact");
  assert.equal(placeOf("desktop/src/shell/trayModel.test.mjs"), null);
  assert.equal(placeOf("desktop/src/shell/trayModel.d.mts"), null);
  assert.equal(placeOf("desktop/src/shell/Sidebar.tsx"), null, "a component is outside the meter by rule");
  assert.equal(placeOf("scripts/coverage/coverageModel.mjs"), null, "the scripts are not measured yet");
  assert.equal(placeOf("website/site.js"), null);
});

function rel(byPath) {
  const out = new Map();
  for (const [p, lines] of byPath) {
    const r = relativize(p, ROOT);
    if (r) out.set(r, lines);
  }
  return out;
}

test("an excluded file is not totalled, and a test source the tool reported is dropped by placement", () => {
  const stats = fileStats(rel(mergeLcov([parseLcov(LLVM), parseLcov(NODE)])));
  assert.deepEqual(
    stats.map((f) => [f.path, f.tree, f.found, f.hit]),
    [
      ["crates/bisa-core/src/id.rs", "crates/bisa-core", 4, 3],
      ["desktop/src/shell/trayModel.mjs", "desktop/src/shell", 3, 2],
    ]
  );
  const kept = applyExclusions(stats, ["crates/bisa-core/src/id.rs"]);
  assert.deepEqual(
    kept.map((f) => f.path),
    ["desktop/src/shell/trayModel.mjs"]
  );
});

test("a desktop model no test loaded counts as unmeasured with its physical lines, and a crate file is never invented", () => {
  const stats = fileStats(rel(parseLcov(NODE)));
  const onDisk = ["desktop/src/shell/trayModel.mjs", "desktop/src/views/_goal/goalLinks.mjs", "desktop/src/shell/x.test.mjs", "desktop/src/shell/x.d.mts", "crates/bisa-core/src/lib.rs"];
  const all = completeDesktop(stats, onDisk, (p) => (p.endsWith("goalLinks.mjs") ? 15 : 99));
  assert.deepEqual(
    all.map((f) => [f.path, f.found, f.hit, f.unmeasured]),
    [
      ["desktop/src/shell/trayModel.mjs", 3, 2, false],
      ["desktop/src/views/_goal/goalLinks.mjs", 15, 0, true],
    ]
  );
});

test("a tree's percentage is floored to a tenth by integer arithmetic, so a baseline written from a run holds against the same run", () => {
  assert.equal(percent(2, 3), 66.6);
  assert.equal(percent(29, 100), 29);
  assert.equal(percent(1, 1), 100);
  assert.equal(percent(0, 7), 0);
  assert.equal(percent(0, 0), TARGET, "a file with no coverable line is whole");
  assert.equal(percent(999, 1000), 99.9);
  for (let found = 1; found < 400; found += 1) {
    for (const hit of [0, 1, Math.floor(found / 3), found - 1, found]) {
      const p = percent(hit, found);
      assert.ok(Number.isInteger(p * 10 + 0.0000001) || Math.abs(p * 10 - Math.round(p * 10)) < 1e-9, `${hit}/${found} -> ${p}`);
      assert.ok(p <= (hit / found) * 100 + 1e-9, "never above the true figure");
      assert.ok(p >= (hit / found) * 100 - 0.1 - 1e-9, "never a tenth below it");
    }
  }
});

test("a tree at 100 is every file at 100, and a tree with no coverable line is not a row", () => {
  const totals = treeTotals([
    { path: "crates/a/src/x.rs", tree: "crates/a", found: 10, hit: 10, unmeasured: false },
    { path: "crates/a/src/y.rs", tree: "crates/a", found: 4, hit: 4, unmeasured: false },
    { path: "crates/b/src/x.rs", tree: "crates/b", found: 10, hit: 10, unmeasured: false },
    { path: "crates/b/src/y.rs", tree: "crates/b", found: 1, hit: 0, unmeasured: false },
    { path: "crates/c/src/lib.rs", tree: "crates/c", found: 0, hit: 0, unmeasured: false },
  ]);
  assert.deepEqual([...totals.keys()], ["crates/a", "crates/b"]);
  assert.equal(totals.get("crates/a").pct, 100);
  assert.equal(totals.get("crates/b").pct, 90.9, "one bare line anywhere and the tree is under the target");
  assert.deepEqual(
    totals.get("crates/b").files.map((f) => [f.path, f.pct]),
    [
      ["crates/b/src/y.rs", 0],
      ["crates/b/src/x.rs", 100],
    ],
    "files ascend"
  );
});

const TOTALS = treeTotals([
  { path: "crates/a/src/x.rs", tree: "crates/a", found: 10, hit: 9, unmeasured: false },
  { path: "crates/b/src/x.rs", tree: "crates/b", found: 10, hit: 10, unmeasured: false },
  { path: "crates/c/src/x.rs", tree: "crates/c", found: 10, hit: 5, unmeasured: false },
  { path: "desktop/src/d.mjs", tree: "desktop/src", found: 10, hit: 7, unmeasured: false },
]);
const BASELINE = { "crates/a": 90, "crates/b": 99.9, "crates/c": 60, "crates/z": 100 };

test("the verdicts: held, rose, fell, new, gone — and a partial judge sees only the trees it was asked about", () => {
  const full = judge(TOTALS, BASELINE);
  assert.deepEqual(
    full.rows.map((r) => [r.tree, r.verdict]),
    [
      ["crates/a", "held"],
      ["crates/b", "rose"],
      ["crates/c", "fell"],
      ["crates/z", "gone"],
      ["desktop/src", "new"],
    ]
  );
  assert.equal(full.ok, false);
  const some = judge(TOTALS, BASELINE, ["crates/a", "crates/b"]);
  assert.deepEqual(
    some.rows.map((r) => [r.tree, r.verdict]),
    [
      ["crates/a", "held"],
      ["crates/b", "rose"],
    ]
  );
  assert.equal(some.ok, true, "a rise passes; what was not asked about is not judged");
  assert.equal(judge(TOTALS, { ...BASELINE, "crates/c": 50, "desktop/src": 70 }, null).rows.find((r) => r.tree === "crates/z").verdict, "gone");
  const held = judge(treeTotals([{ path: "crates/a/src/x.rs", tree: "crates/a", found: 3, hit: 2, unmeasured: false }]), { "crates/a": 66.6 });
  assert.equal(held.ok, true, "the floor and the file agree");
});

test("the next baseline raises and adds, never lowers, and drops a gone tree on a full judge alone", () => {
  const full = nextBaseline(BASELINE, TOTALS);
  assert.deepEqual(full, { "crates/a": 90, "crates/b": 100, "crates/c": 60, "desktop/src": 70 });
  assert.deepEqual(Object.keys(full), ["crates/a", "crates/b", "crates/c", "desktop/src"], "sorted");
  const some = nextBaseline(BASELINE, TOTALS, ["crates/b", "crates/c"]);
  assert.deepEqual(some, { "crates/a": 90, "crates/b": 100, "crates/c": 60, "crates/z": 100 }, "a partial write keeps every other key, the gone one included");
  assert.deepEqual(nextBaseline({}, TOTALS), { "crates/a": 90, "crates/b": 100, "crates/c": 50, "desktop/src": 70 }, "the first write is the reading");
});

test("the report names what fell, what rose and what is new; the gate's word is the last line; a file list ascends with the unmeasured first", () => {
  const text = formatReport(judge(TOTALS, BASELINE), {
    totals: treeTotals([
      { path: "desktop/src/a.mjs", tree: "desktop/src", found: 10, hit: 10, unmeasured: false },
      { path: "desktop/src/b.mjs", tree: "desktop/src", found: 10, hit: 1, unmeasured: false },
      { path: "desktop/src/c.mjs", tree: "desktop/src", found: 12, hit: 0, unmeasured: true },
    ]),
    files: ["desktop/src", "crates/none"],
  });
  // The columns are padded to line up; the pins are about the words and
  // their order, so runs of spaces are squeezed before matching.
  const squeezed = text.replace(/ +/g, " ");
  assert.ok(squeezed.startsWith(`coverage: 5 trees against ${BASELINE_FILE}\n`));
  assert.ok(squeezed.includes("\n crates/a 90.0 % 9 / 10 baseline 90.0 held\n"));
  assert.ok(squeezed.includes("\n crates/b 100.0 % 10 / 10 baseline 99.9 rose — `just coverage-write` holds it\n"));
  assert.ok(squeezed.includes("\n crates/c 50.0 % 5 / 10 baseline 60.0 FELL\n"));
  assert.ok(squeezed.includes("\n crates/z — 0 / 0 baseline 100.0 GONE — no tree measured; `just coverage-write` drops it\n"));
  assert.ok(squeezed.includes("\n desktop/src 70.0 % 7 / 10 baseline — new — `just coverage-write` adds it\n"));
  assert.ok(squeezed.includes("\n1 fell (crates/c by 10.0), 1 rose, 1 new, 1 gone; 1 of 5 at 100. The end is 100 in every row.\ncoverage: the gate does not hold\n"));
  assert.ok(
    squeezed.includes("\ndesktop/src: 3 files, ascending\n desktop/src/c.mjs 0.0 % 0 / 12 (no test loads it)\n desktop/src/b.mjs 10.0 % 1 / 10\n desktop/src/a.mjs 100.0 % 10 / 10\n")
  );
  assert.ok(squeezed.includes("\ncrates/none: no measured file\n"));
  assert.ok(text.includes("  crates/a       90.0 %        9 / 10       baseline 90.0    held\n"), "the columns line up");
  const whole = formatReport(judge(TOTALS, nextBaseline({}, TOTALS)));
  assert.ok(whole.endsWith("\nevery tree held; 1 of 4 at 100. The end is 100 in every row.\ncoverage: the gate holds\n"));
  const raised = formatReport(judge(TOTALS, nextBaseline(BASELINE, TOTALS)));
  assert.ok(raised.includes("\n1 fell (crates/c by 10.0); 1 of 4 at 100."), "a baseline raised from a fallen run keeps the old figure, so the fall stands");
  const report = formatReport(judge(TOTALS, {}, ["crates/a"]), { verdicts: false }).replace(/ +/g, " ");
  assert.equal(report, "coverage: 1 trees measured by their own suites (a report, not a verdict)\n crates/a 90.0 % 9 / 10\n");
});

test("the bare lines of a file read as ranges", () => {
  assert.deepEqual(
    bareRanges(
      new Map([
        [1, 1],
        [2, 0],
        [3, 0],
        [4, 0],
        [5, 2],
        [9, 0],
        [10, 0],
        [12, 0],
      ])
    ),
    ["2-4", "9-10", "12"]
  );
  assert.deepEqual(bareRanges(new Map([[1, 1]])), []);
});

// ---------------------------------------------------------------------------
// What a Rust source excuses on its own.
// ---------------------------------------------------------------------------

const RUST = [
  "pub fn shipped() -> u8 {",            // 1
  '    let s = "}"; // a brace in a string',
  "    let c = '{';",                     // 3
  "    /* a comment } with a brace */",
  "    let r = r#\"}\"#;",                // 5
  "    1",
  "}",                                   // 7
  "",
  "/// #[cfg(test)] in a doc comment is words",
  "#[cfg(test)]",                        // 10
  "#[allow(dead_code)]",
  "fn helper(a: u8) -> u8 {",            // 12
  "    a /* { */ + 1",
  "}",                                   // 14
  "",
  "#[cfg(test)]",                        // 16
  "mod verbs;",                          // 17
  "",
  "#[cfg(test)]",                        // 19
  "mod tests {",                         // 20
  "    use super::*;",
  "    fn f() { let s = \"{\"; let c = '}'; assert!(s != \"\"); }",
  "    #[test]",
  "    fn g() {",
  "        let lifetime: &'static str = \"x\";",
  "        let _ = lifetime;",
  "    }",
  "}",                                   // 28
  "",
  "pub fn after() -> u8 { 2 }",          // 30
  "#[cfg(test)]",                        // 31
  "use std::fmt;",                       // 32
  "pub fn last() {}",                    // 33
].join("\n");

test("the braces of a Rust source are counted as code only: a string, a character, a comment and a raw string open nothing", () => {
  const events = codeBraces(RUST).map((b) => `${b.line}${b.ch}`);
  assert.deepEqual(events.slice(0, 6), ["1{", "2;", "3;", "5;", "7}", "12{"]);
  assert.ok(!events.includes("2{") && !events.includes("3{") && !events.includes("4}") && !events.includes("5}"), "nothing inside a literal counts");
  assert.ok(events.includes("17;") && events.includes("20{") && events.includes("28}") && events.includes("32;"));
  assert.deepEqual(codeBraces("/* nested /* inner */ still } */ {").map((b) => b.ch), ["{"], "a block comment nests");
  assert.deepEqual(codeBraces("let b = b\"{\"; let r = br\"}\"; {").map((b) => `${b.line}${b.ch}`), ["1;", "1;", "1{"], "byte and raw byte strings");
  assert.deepEqual(codeBraces("let c = '\\'';{").map((b) => b.ch), [";", "{"], "an escaped quote in a character");
  assert.deepEqual(
    codeBraces('let s = "a \\\nb";\n{').map((b) => `${b.line}${b.ch}`),
    ["2;", "3{"],
    "a newline escaped inside a string is still a line"
  );
});

test("an item under #[cfg(test)] is a span from the attribute to its closing brace or its semicolon, whatever its strings and comments say", () => {
  assert.deepEqual(testItemSpans(RUST), [
    [10, 14],
    [16, 17],
    [19, 28],
    [31, 32],
  ]);
  assert.deepEqual(testItemSpans("fn a() {}\n"), []);
  assert.deepEqual(testItemSpans("#[cfg(test)]\nmod tests {\n  fn f() {\n  }\n"), [[1, 4]], "a block the file ends inside runs to the file's end");
  assert.deepEqual(testItemSpans("#[cfg(any(test, feature = \"mock\"))]\nmod shared {}\n"), [], "only the bare attribute: code shared with a feature stays measured");
});

test("a marker excuses its line or its block, says why in words, and a block left open or a marker without a reason is a fault", () => {
  const src = [
    "let a = 1;",
    "let b = 2; // LCOV_EXCL_LINE: unreachable by the invariant held by a_test_name",
    "// LCOV_EXCL_START — this arm needs a broken TLS backend, which the shipped one is not",
    "let c = 3;",
    "let d = 4;",
    "// LCOV_EXCL_STOP",
    "let e = 5;",
  ].join("\n");
  const marked = excludedLines(src);
  assert.deepEqual([...marked.lines].sort((x, y) => x - y), [2, 3, 4, 5, 6]);
  assert.deepEqual(
    marked.excused.map((e) => [e.line, e.reason]),
    [
      [2, "unreachable by the invariant held by a_test_name"],
      [3, "this arm needs a broken TLS backend, which the shipped one is not"],
    ]
  );
  assert.deepEqual(marked.faults, []);
  assert.deepEqual(excludedLines("x // LCOV_EXCL_LINE\n").faults, ["line 1: LCOV_EXCL_LINE says no reason"]);
  assert.deepEqual(excludedLines("x // LCOV_EXCL_LINE: two words\n").faults, ["line 1: LCOV_EXCL_LINE says no reason"]);
  assert.deepEqual(excludedLines("// LCOV_EXCL_START: a reason of words\nx\n").faults, ["line 1: LCOV_EXCL_START never stopped"]);
  assert.deepEqual(excludedLines("// LCOV_EXCL_STOP\n").faults, ["line 1: LCOV_EXCL_STOP with no block open"]);
});

test("a file's counts lose its test items and its marked lines and keep the rest as they were", () => {
  const lines = new Map([...Array(33).keys()].map((i) => [i + 1, i % 2]));
  const src = RUST.replace("pub fn after() -> u8 { 2 }", "pub fn after() -> u8 { 2 } // LCOV_EXCL_LINE: an arm nobody can reach by the invariant");
  const taken = withoutExcused(lines, src);
  for (const k of [10, 11, 12, 13, 14, 16, 17, 19, 20, 25, 28, 30, 31, 32]) assert.ok(!taken.lines.has(k), `line ${k} is out`);
  for (const k of [1, 2, 3, 7, 33]) assert.equal(taken.lines.get(k), lines.get(k), `line ${k} is as it was`);
  assert.equal(taken.testLines, 19, "the four spans: 5 + 2 + 10 + 2 lines");
  assert.equal(taken.edgeLines, 0, "no line here is only a `?`");
  assert.equal(taken.excused.length, 1);
  assert.deepEqual(taken.faults, []);
});

// ---------------------------------------------------------------------------
// The committed files, held to the tree.
// ---------------------------------------------------------------------------

test("every excluded path exists, would otherwise be measured, is listed once in order and says why", () => {
  const list = JSON.parse(read("scripts/coverage/exclusions.json"));
  assert.ok(Array.isArray(list) && list.length > 0, "a list of entries");
  const paths = list.map((e) => e.path);
  assert.deepEqual(paths, [...new Set(paths)].sort(), "once each, sorted by path");
  for (const entry of list) {
    assert.deepEqual(Object.keys(entry).sort(), ["path", "reason"], `${entry.path}: a path and a reason, nothing else`);
    assert.ok(existsSync(join(root, entry.path)) && statSync(join(root, entry.path)).isFile(), `${entry.path} is a file in the tree`);
    assert.ok(placeOf(entry.path) !== null, `${entry.path} would otherwise be measured — a path nobody measures needs no excuse`);
    assert.ok(entry.reason.trim().split(/\s+/).length >= 3, `${entry.path} says why in words`);
  }
});

test("every baseline key is a tree on disk, in order, with a percentage of at most one decimal", (t) => {
  const file = join(root, BASELINE_FILE);
  if (!existsSync(file)) {
    t.diagnostic(`${BASELINE_FILE} is not written yet — \`just coverage-write\` after a full run`);
    return;
  }
  const baseline = JSON.parse(readFileSync(file, "utf8"));
  const keys = Object.keys(baseline);
  assert.ok(keys.length > 0, "at least one tree");
  assert.deepEqual(keys, [...keys].sort(), "sorted by tree");
  for (const [tree, value] of Object.entries(baseline)) {
    assert.ok(typeof value === "number" && value >= 0 && value <= TARGET, `${tree}: a percentage`);
    assert.ok(Math.abs(value * 10 - Math.round(value * 10)) < 1e-9, `${tree}: at most one decimal`);
    let dir;
    if (/^crates\/[^/]+$/.test(tree)) dir = join(root, tree, "src");
    else if (tree === "desktop/src-tauri") dir = join(root, tree, "src");
    else if (tree === "desktop/src" || tree.startsWith("desktop/src/")) dir = join(root, tree);
    else assert.fail(`${tree}: not a crate, the shell or a desktop directory`);
    assert.ok(existsSync(dir) && statSync(dir).isDirectory(), `${tree}: a tree on disk`);
    assert.equal(placeOf(tree === "desktop/src-tauri" ? `${tree}/src/x.rs` : tree.startsWith("crates/") ? `${tree}/src/x.rs` : `${tree}/x.mjs`), tree, `${tree}: the tree a file there is placed in`);
  }
});

test("a line that is only the closing of a `?` is the error edge and leaves the meter", () => {
  const source = [
    "fn f() -> R {",           // 1
    "    let a = g(",          // 2
    "        1,",              // 3
    "    )?;",                 // 4  the edge
    "    let b = h(|x| {",     // 5
    "        x",               // 6
    "    })?;",                // 7  the edge
    "    k(",                  // 8
    "        a,",              // 9
    "    )?",                  // 10 the edge, as a tail expression
    "    .ok_or(e)?;",         // 11 not: it names a method
    "    m(n(",                // 12
    "        o,",              // 13
    "    )?);",                // 14 the edge, inside a call
    "    Ok(p(",               // 15
    "        q,",              // 16
    "    )?)",                 // 17 the edge, inside a tail
    "}",                       // 18
  ].join("\n");
  assert.deepEqual([...questionEdgeLines(source)], [4, 7, 10, 14, 17]);
  const lines = new Map([[2, 1], [3, 1], [4, 0], [7, 0], [10, 0], [11, 1], [14, 0], [17, 0]]);
  const taken = withoutExcused(lines, source);
  assert.equal(taken.edgeLines, 5);
  assert.deepEqual([...taken.lines.keys()], [2, 3, 11]);
});

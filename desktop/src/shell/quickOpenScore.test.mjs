import { strict as assert } from "node:assert";
import { test } from "node:test";

import { readFileSync } from "node:fs";

import { MAX_ITEMS, MAX_ROWS, SECTION_CAPS, admit, groupItems, itemMatches, listed, paletteNeedle, parsePrefix, patchIndex, rankPaths, scorePath } from "./quickOpenScore.mjs";

test("every query character must appear in order", () => {
  assert.equal(scorePath("main", "src/main.rs") !== null, true);
  assert.equal(scorePath("mian", "src/main.rs"), null);
  assert.equal(scorePath("zzz", "src/main.rs"), null);
});

test("the basename, word starts and runs win over scattered matches", () => {
  const ranked = rankPaths("main", ["src/domain/mailing.rs", "src/main.rs", "docs/maintenance.md"]);
  assert.equal(ranked[0].path, "src/main.rs");
  const exact = rankPaths("lib.rs", ["src/lib.rs", "src/library.rs", "tests/lib.rs.bak"]);
  assert.equal(exact[0].path, "src/lib.rs");
});

test("an empty query lists the first paths; the limit caps the section", () => {
  const paths = Array.from({ length: 50 }, (_, i) => `f${i}.rs`);
  assert.equal(rankPaths("", paths).length, 12);
  assert.equal(rankPaths("f", paths, 5).length, 5);
});

test("prefixes narrow the palette", () => {
  assert.deepEqual(parsePrefix(">theme"), { prefix: ">", query: "theme" });
  assert.deepEqual(parsePrefix(" : 42"), { prefix: ":", query: "42" });
  assert.deepEqual(parsePrefix("#cart"), { prefix: "#", query: "cart" });
  assert.deepEqual(parsePrefix("main.rs"), { prefix: null, query: "main.rs" });
});

test("a file_changed frame patches the cached index without a refetch", () => {
  const idx = ["a.rs", "b.rs"];
  assert.deepEqual(patchIndex(idx, { kind: "created", path: "c.rs" }), ["a.rs", "b.rs", "c.rs"]);
  assert.deepEqual(patchIndex(idx, { kind: "removed", path: "a.rs" }), ["b.rs"]);
  assert.deepEqual(patchIndex(idx, { kind: "renamed", path: "z.rs", from: "a.rs" }), ["b.rs", "z.rs"]);
  assert.equal(patchIndex(idx, { kind: "modified", path: "a.rs" }), idx, "an unchanged index is the same array");
});

test("first results on 100k paths land well inside the budget", () => {
  const paths = [];
  for (let i = 0; i < 100_000; i++) {
    paths.push(`crates/pkg${i % 97}/src/module_${i % 1013}/file_${i}.rs`);
  }
  // Best of three: `node --test` runs every file in parallel, and a single
  // sample taken while the machine is busy measures the machine, not the
  // scorer. The budget is 50 ms on the reference machine; the test allows
  // ten times that and still catches an algorithm that went quadratic.
  let ms = Infinity;
  let ranked = [];
  for (let run = 0; run < 3; run++) {
    const t0 = performance.now();
    ranked = rankPaths("mod file 42", paths);
    ms = Math.min(ms, performance.now() - t0);
  }
  assert.ok(ranked.length > 0);
  assert.ok(ms < 500, `scoring 100k paths took ${ms.toFixed(1)} ms`);
});

test("the palette's needle: the words after a prefix in a prefix mode, the whole line in all — trimmed and folded", () => {
  assert.equal(paletteNeedle("  Theme ", "all"), "theme");
  assert.equal(paletteNeedle(">Theme", "commands"), "theme", "the prefix goes");
  assert.equal(paletteNeedle(">Theme", "all"), ">theme", "in all the prefix is part of the words");
  assert.equal(paletteNeedle(":42", "line"), "42");
  assert.equal(paletteNeedle("", "all"), "");
});

test("a row matches on its label, its hint and its hidden keywords together; an empty needle matches every row", () => {
  const row = { label: "Toggle word wrap", hint: "Editor", keywords: "soft wrap lines" };
  assert.ok(itemMatches(row, ""));
  assert.ok(itemMatches(row, "word wrap") && itemMatches(row, "editor") && itemMatches(row, "soft"));
  assert.ok(itemMatches(row, "editor wrap") === false, "the words are one needle in order, not several");
  assert.ok(!itemMatches({ label: "Save" }, "wrap"), "no hint, no keywords: the label alone");
});

test("a section admits rows to its cap and no further, so one category cannot flood the list; an unnamed section takes the row cap", () => {
  const counts = new Map();
  for (let i = 0; i < SECTION_CAPS.Terminals; i++) assert.ok(admit(counts, "Terminals"));
  assert.equal(admit(counts, "Terminals"), false);
  assert.equal(counts.get("Terminals"), SECTION_CAPS.Terminals, "a refused row is not counted");
  for (let i = 0; i < MAX_ROWS; i++) assert.ok(admit(counts, "Search"));
  assert.equal(admit(counts, "Search"), false);
  assert.ok(MAX_ITEMS < MAX_ROWS, "the list is shorter than any one section could be");
});

test("rows group by section in the order the sections were first met, and the flat list is the cursor's", () => {
  const items = [
    { key: "a", group: "Commands" },
    { key: "b", group: "Files" },
    { key: "c", group: "Commands" },
  ];
  const { groups, flat } = groupItems(items);
  assert.deepEqual(groups.map(([g, rows]) => [g, rows.map((r) => r.key)]), [["Commands", ["a", "c"]], ["Files", ["b"]]]);
  assert.deepEqual(flat.map((r) => r.key), ["a", "c", "b"], "the cursor walks the sections, not the build order");
  assert.deepEqual(groupItems([]), { groups: [], flat: [] });
});

test("a folder that goes or moves takes every file under it, and its own name is never a file", () => {
  const idx = ["README.md", "src/a.rs", "src/lib/b.rs", "srcs/keep.rs", "tests/t.rs"];
  assert.deepEqual(patchIndex(idx, { kind: "removed", path: "src" }), ["README.md", "srcs/keep.rs", "tests/t.rs"], "a name that merely begins the same is another folder");
  assert.deepEqual(patchIndex(idx, { kind: "renamed", from: "src", path: "core", dir: true }), ["README.md", "core/a.rs", "core/lib/b.rs", "srcs/keep.rs", "tests/t.rs"]);
  assert.deepEqual(patchIndex(idx, { kind: "renamed", from: "src", path: "core" }), ["README.md", "core/a.rs", "core/lib/b.rs", "srcs/keep.rs", "tests/t.rs"], "a frame that does not say is read from what it moved");
  assert.deepEqual(patchIndex(idx, { kind: "renamed", from: "src/lib", path: "lib", dir: true }), ["README.md", "lib/b.rs", "src/a.rs", "srcs/keep.rs", "tests/t.rs"]);
  assert.equal(patchIndex(idx, { kind: "removed", path: "nowhere" }), idx, "what was never held is nothing to forget");
  assert.equal(patchIndex(idx, { kind: "created", path: "empty", dir: true }) === null, true, "a folder made whole may hold files nobody named");
  assert.equal(patchIndex(idx, { kind: "created", path: "src", dir: true }), idx, "one whose files are already held changes nothing");
  assert.ok(!(patchIndex(idx, { kind: "created", path: "docs/x.md" }) ?? []).includes("docs"), "a file's folder is never listed");
});

test("a frame that says more than a list can take asks for the list: a rename out of what was hidden, or from nowhere", () => {
  const idx = ["a.rs"];
  assert.equal(patchIndex(idx, { kind: "renamed", from: "target/hidden.rs", path: "shown.rs" }), null);
  assert.equal(patchIndex(idx, { kind: "renamed", from: null, path: "shown.rs" }), null);
  assert.equal(patchIndex(idx, { kind: "renamed", path: "shown.rs" }), null);
  assert.equal(patchIndex(idx, { kind: "invented_later", path: "a.rs" }), idx, "a kind nobody knows changes nothing");
  assert.deepEqual(patchIndex(idx, { kind: "created", path: "a.rs" }), idx, "made twice is held once");
  assert.deepEqual(patchIndex(["b b.rs", "ünï/ç.rs"], { kind: "renamed", from: "ünï", path: "uni code", dir: true }), ["b b.rs", "uni code/ç.rs"], "spaces and letters of any alphabet are a name like any other");
});


test("what the node's index leaves out is never put in: a path the ignore rules match, a hidden name, git's own files", () => {
  const idx = ["README.md", "src/a.rs"];
  assert.equal(listed("src/b.rs", false), true);
  assert.equal(listed("target/debug/app", true), false, "the frame says the ignore rules match it");
  for (const hidden of [".env", ".git/HEAD", ".git/refs/heads/main", "src/.cache/x", ".github/workflows/ci.yml"]) assert.equal(listed(hidden, false), false, hidden);
  assert.equal(listed("src/a.b.rs", false), true, "a dot inside a name hides nothing");
  // A build writes a thousand files into `target/`: a thousand frames, and the same list.
  let built = idx;
  for (let i = 0; i < 1000; i += 1) built = patchIndex(built, { kind: "created", path: `target/debug/deps/lib${i}.rlib`, ignored: true });
  assert.equal(built, idx, "the same array: nothing to redraw, nothing kept");
  assert.equal(patchIndex(idx, { kind: "created", path: "node_modules", dir: true, ignored: true }), idx, "an ignored folder made whole asks for no re-read");
  assert.equal(patchIndex(idx, { kind: "created", path: ".git/index", ignored: false }), idx, "a commit's frame adds no row");
  assert.equal(patchIndex(idx, { kind: "created", path: ".cache", dir: true }), idx);
  // Removed, whatever it was: a list that never held it is unchanged.
  assert.equal(patchIndex(idx, { kind: "removed", path: "target", ignored: true }), idx);
});

test("a file renamed out of sight leaves the list, and one renamed between two hidden places changes nothing", () => {
  const idx = ["README.md", "src/a.rs", "src/lib/b.rs"];
  assert.deepEqual(patchIndex(idx, { kind: "renamed", from: "src/a.rs", path: "target/a.rs", ignored: true }), ["README.md", "src/lib/b.rs"]);
  assert.deepEqual(patchIndex(idx, { kind: "renamed", from: "src/lib", path: ".trash/lib", dir: true }), ["README.md", "src/a.rs"], "a folder moved under a hidden name takes its files with it");
  assert.equal(patchIndex(idx, { kind: "renamed", from: "target/x.o", path: "target/y.o", ignored: true }), idx);
  assert.equal(patchIndex(idx, { kind: "renamed", from: null, path: "target/y.o", ignored: true }), idx, "from nowhere to out of sight: nothing came into view");
  assert.equal(patchIndex(idx, { kind: "renamed", from: "target/hidden.rs", path: "shown.rs", ignored: false }), null, "into view from what the list never held: read again");
});

test("the store hands the patch the frame whole — whether the path is a folder, whether it is ignored", () => {
  const store = readFileSync(new URL("./pathIndexStore.ts", import.meta.url), "utf8");
  assert.ok(store.includes("{ kind: p.kind, path: p.path, from: p.from, dir: p.dir, ignored: p.ignored }"), "a frame cut short once listed a new folder as a file, and never re-read a tree copied in");
});

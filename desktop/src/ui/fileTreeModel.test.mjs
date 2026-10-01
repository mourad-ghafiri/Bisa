/**
 * The file tree's arithmetic and its state machine.
 *
 * There is no jsdom and no React testing library in this repo, so nothing
 * here renders. That is not a gap being worked around — it is why the model
 * is a separate module in the first place. Every way a tree can be *wrong*
 * that a person would actually notice is a decision about data: which
 * directory a path belongs to, which listings still need fetching, what
 * happens to the siblings of a folder that would not open, what a key press
 * means at the cursor. All of that is here. What is not here is whether the
 * chevron points the right way.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  ROOT,
  activate,
  bodyRows,
  formatSize,
  initialState,
  isDescendant,
  isImmediateChild,
  joinPath,
  openFolderPaths,
  parentPath,
  pendingLoads,
  rangeBetween,
  reduce,
  sortEntries,
  targetsOf,
  visibleRows,
  withDraft,
} from "./fileTreeModel.mjs";

test("withDraft splices a new-entry draft under its target folder, one level deeper", () => {
  const rows = [
    { kind: "entry", id: "src", path: "src", depth: 0, entry: { name: "src", dir: true }, expanded: true },
    { kind: "entry", id: "src/a.ts", path: "src/a.ts", depth: 1, entry: { name: "a.ts", dir: false }, expanded: false },
    { kind: "entry", id: "README", path: "README", depth: 0, entry: { name: "README", dir: false }, expanded: false },
  ];
  // At the root: the draft leads.
  const atRoot = withDraft(rows, { dir: "", kind: "file" });
  assert.equal(atRoot[0].kind, "draft");
  assert.equal(atRoot[0].depth, 0);
  assert.equal(atRoot[0].entryKind, "file");
  assert.equal(atRoot.length, rows.length + 1);
  // Under a folder: right after the folder row, one level deeper.
  const inSrc = withDraft(rows, { dir: "src", kind: "dir" });
  assert.deepEqual(
    inSrc.map((r) => `${r.kind}:${r.path || r.dir}`),
    ["entry:src", "draft:src", "entry:src/a.ts", "entry:README"],
  );
  assert.equal(inSrc[1].depth, 1, "one level under its folder");
  // No draft, or a folder not visible: unchanged / falls back to the top.
  assert.equal(withDraft(rows, null), rows);
  assert.equal(withDraft(rows, { dir: "gone", kind: "file" })[0].kind, "draft");
});

const HERE = dirname(fileURLToPath(import.meta.url));

/** A `FileEntry` shaped like the node's, with only the fields under test set. */
function entry(path, { dir = false, kind, size = null, symlink = false } = {}) {
  return {
    path,
    name: path.slice(path.lastIndexOf("/") + 1),
    kind: kind ?? (dir ? "dir" : "file"),
    dir,
    symlink,
    size,
    modified: null,
  };
}

/** A state with these listings already loaded and these directories open. */
function loaded(listings, open = []) {
  let s = initialState();
  for (const [path, entries] of Object.entries(listings)) {
    s = reduce(s, { type: "loaded", path, entries, truncated: false });
  }
  for (const path of open) s = reduce(s, { type: "expand", path });
  return s;
}

// --- paths -----------------------------------------------------------------

test("joining onto the root does not produce an absolute path", () => {
  // `${base}/${name}` with an empty base gives "/notes", which the node reads
  // as absolute and refuses — and only ever at the top level.
  assert.equal(joinPath(ROOT, "notes"), "notes");
  assert.equal(joinPath("artifacts", "patch.diff"), "artifacts/patch.diff");
  assert.equal(joinPath("artifacts/", "patch.diff"), "artifacts/patch.diff");
});

test("a top-level entry's parent is the root itself", () => {
  assert.equal(parentPath("artifacts"), ROOT);
  assert.equal(parentPath("artifacts/run-1/patch.diff"), "artifacts/run-1");
});

test("an entry's path is always relative to the root, so its parent is the listing it came from", () => {
  // The invariant the whole flat state map rests on: the node returns paths
  // relative to `root`, never to the sub-path that was listed, so handing an
  // entry's path back as `?path=` addresses exactly that entry.
  const listedPath = "workstreams/feature-x";
  const children = [
    entry("workstreams/feature-x/src", { dir: true }),
    entry("workstreams/feature-x/README.md"),
  ];
  for (const child of children) {
    assert.equal(parentPath(child.path), listedPath);
    assert.ok(isImmediateChild(listedPath, child.path));
  }
});

test("a grandchild is a descendant but not an immediate child", () => {
  assert.ok(isDescendant("artifacts", "artifacts/run-1/patch.diff"));
  assert.equal(isImmediateChild("artifacts", "artifacts/run-1/patch.diff"), false);
});

test("a sibling with a shared name prefix is not underneath anything", () => {
  // "artifacts-old" starts with "artifacts", and a `startsWith` without the
  // separator would file it under the wrong parent.
  assert.equal(isDescendant("artifacts", "artifacts-old/notes.md"), false);
  assert.equal(isImmediateChild("artifacts", "artifacts-old"), false);
  assert.equal(parentPath("artifacts-old"), ROOT);
});

test("the root contains everything but itself", () => {
  assert.ok(isDescendant(ROOT, "anything/at/all"));
  assert.equal(isDescendant(ROOT, ROOT), false);
});

// --- sizes -----------------------------------------------------------------

test("a directory's absent size renders as nothing, never as zero bytes", () => {
  // The node sends `null` for a directory. "0 B" would be a claim that the
  // folder is empty, which is a different fact and often a false one.
  assert.equal(formatSize(null), "");
  assert.equal(formatSize(undefined), "");
  assert.equal(formatSize(0), "0 B");
});

test("sizes are formatted at one decimal below ten and none above", () => {
  assert.equal(formatSize(512), "512 B");
  assert.equal(formatSize(1023), "1023 B");
  assert.equal(formatSize(1024), "1.0 KB");
  assert.equal(formatSize(1536), "1.5 KB");
  assert.equal(formatSize(10 * 1024), "10 KB");
  assert.equal(formatSize(256 * 1024), "256 KB");
  assert.equal(formatSize(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
});

test("a nonsense size renders as nothing rather than as NaN", () => {
  assert.equal(formatSize(-1), "");
  assert.equal(formatSize(Number.NaN), "");
  assert.equal(formatSize("12"), "");
});

// --- ordering --------------------------------------------------------------

test("directories sort above files whatever they are called", () => {
  const sorted = sortEntries([
    entry("a.txt"),
    entry("zebra", { dir: true }),
    entry("b.txt"),
  ]);
  assert.deepEqual(
    sorted.map((e) => e.name),
    ["zebra", "a.txt", "b.txt"],
  );
});

test("numbered files sort the way a person counts", () => {
  const sorted = sortEntries([entry("run-10.log"), entry("run-9.log"), entry("run-1.log")]);
  assert.deepEqual(
    sorted.map((e) => e.name),
    ["run-1.log", "run-9.log", "run-10.log"],
  );
});

test("names that differ only in case land next to each other, in a stable order", () => {
  // Left to the default collation these end up at opposite ends of the list,
  // and a refresh that re-sorts differently looks like the files moved.
  const once = sortEntries([entry("README"), entry("readme"), entry("Readme")]);
  const again = sortEntries([entry("readme"), entry("Readme"), entry("README")]);
  assert.deepEqual(once.map((e) => e.path), again.map((e) => e.path));
});

test("sorting does not mutate the caller's array", () => {
  const original = [entry("b"), entry("a")];
  sortEntries(original);
  assert.deepEqual(original.map((e) => e.name), ["b", "a"]);
});

// --- loading -------------------------------------------------------------

test("a fresh tree asks for the root and nothing else", () => {
  assert.deepEqual(pendingLoads(initialState()), [ROOT]);
});

test("a loaded root asks for nothing until a directory is opened", () => {
  const s = loaded({ [ROOT]: [entry("results", { dir: true, kind: "result" })] });
  assert.deepEqual(pendingLoads(s), []);
  assert.deepEqual(pendingLoads(reduce(s, { type: "expand", path: "results" })), ["results"]);
});

test("opening a directory fetches it once, not on every render", () => {
  let s = loaded({ [ROOT]: [entry("artifacts", { dir: true })] }, ["artifacts"]);
  s = reduce(s, { type: "loading", path: "artifacts" });
  assert.deepEqual(pendingLoads(s), []);
});

test("a directory hidden inside a collapsed parent is not fetched", () => {
  // `collapse` deliberately keeps its descendants' open bits so reopening
  // restores the shape. Loading from `open` directly would then refetch an
  // invisible subtree on every poll, forever.
  let s = loaded(
    {
      [ROOT]: [entry("a", { dir: true })],
      a: [entry("a/b", { dir: true })],
    },
    ["a", "a/b"],
  );
  s = reduce(s, { type: "collapse", path: "a" });
  assert.deepEqual(pendingLoads(s), []);
  assert.ok(s.open["a/b"], "the inner folder stays remembered as open");
  assert.deepEqual(pendingLoads(reduce(s, { type: "expand", path: "a" })), ["a/b"]);
});

test("a refresh re-asks for every visible listing without blanking any of them", () => {
  const s = reduce(
    loaded({ [ROOT]: [entry("a", { dir: true })], a: [entry("a/f.txt")] }, ["a"]),
    { type: "invalidate" },
  );
  assert.deepEqual(pendingLoads(s), [ROOT]);
  assert.deepEqual(
    visibleRows(s).map((r) => r.id),
    ["a", "a/f.txt"],
    "the old rows stay on screen while the new listing is in flight",
  );
});

test("changing scope forgets everything the previous tree knew", () => {
  const s = reduce(loaded({ [ROOT]: [entry("a", { dir: true })] }, ["a"]), { type: "reset" });
  assert.deepEqual(s, initialState());
});

test("only the root listing sets the absolute root", () => {
  let s = reduce(initialState(), {
    type: "loaded",
    path: ROOT,
    entries: [],
    truncated: false,
    root: "/w/goals/01",
  });
  s = reduce(s, { type: "loaded", path: "a", entries: [], truncated: false, root: "/nonsense" });
  assert.equal(s.root, "/w/goals/01");
});

// --- rows ------------------------------------------------------------------

test("only opened directories contribute their children", () => {
  const listings = {
    [ROOT]: [entry("a", { dir: true }), entry("z.txt")],
    a: [entry("a/inner.txt")],
  };
  assert.deepEqual(
    visibleRows(loaded(listings)).map((r) => r.id),
    ["a", "z.txt"],
  );
  assert.deepEqual(
    visibleRows(loaded(listings, ["a"])).map((r) => r.id),
    ["a", "a/inner.txt", "z.txt"],
  );
});

test("depth is the nesting level, so a row can indent itself and say its aria-level", () => {
  const s = loaded(
    {
      [ROOT]: [entry("a", { dir: true })],
      a: [entry("a/b", { dir: true })],
      "a/b": [entry("a/b/c.txt")],
    },
    ["a", "a/b"],
  );
  assert.deepEqual(
    visibleRows(s).map((r) => [r.id, r.depth]),
    [
      ["a", 0],
      ["a/b", 1],
      ["a/b/c.txt", 2],
    ],
  );
});

test("one unreadable directory costs its own subtree and nothing else", () => {
  // The failure this exists to prevent: a single top-level error state that
  // throws away every listing that loaded perfectly well.
  let s = loaded(
    {
      [ROOT]: [entry("bad", { dir: true }), entry("good", { dir: true })],
      good: [entry("good/f.txt")],
    },
    ["bad", "good"],
  );
  s = reduce(s, { type: "failed", path: "bad", error: "permission denied" });
  const rows = visibleRows(s);
  assert.deepEqual(
    rows.map((r) => r.kind),
    ["entry", "error", "entry", "entry"],
  );
  assert.equal(rows[1].error, "permission denied");
  assert.equal(rows[3].id, "good/f.txt");
});

test("an empty directory says so instead of looking like it never opened", () => {
  const rows = visibleRows(loaded({ [ROOT]: [entry("a", { dir: true })], a: [] }, ["a"]));
  assert.deepEqual(
    rows.map((r) => r.kind),
    ["entry", "empty"],
  );
});

test("a truncated listing gets a row of its own, so it cannot read as the whole listing", () => {
  let s = initialState();
  s = reduce(s, { type: "loaded", path: ROOT, entries: [entry("f.txt")], truncated: true });
  const rows = visibleRows(s);
  assert.deepEqual(
    rows.map((r) => r.kind),
    ["entry", "truncated"],
  );
});

test("a directory being listed for the first time shows a placeholder, not emptiness", () => {
  let s = loaded({ [ROOT]: [entry("a", { dir: true })] }, ["a"]);
  s = reduce(s, { type: "loading", path: "a" });
  assert.equal(visibleRows(s)[1].kind, "loading");
});

// --- keyboard --------------------------------------------------------------

const NAV = loaded(
  {
    [ROOT]: [entry("a", { dir: true }), entry("z.txt")],
    a: [entry("a/inner.txt")],
  },
  ["a"],
);
const NAV_ROWS = visibleRows(NAV);

test("every row is a TreeRowLike: entries carry a label and whether they open, notices are never focusable", () => {
  assert.deepEqual(
    NAV_ROWS.map((r) => [r.id, r.label ?? null, r.expandable ?? false, r.focusable ?? true]),
    [
      ["a", "a", true, true],
      ["a/inner.txt", "inner.txt", false, true],
      ["z.txt", "z.txt", false, true],
    ],
  );
  const failed = reduce(loaded({ [ROOT]: [entry("bad", { dir: true })] }, ["bad"]), { type: "failed", path: "bad", error: "nope" });
  const notice = visibleRows(failed)[1];
  assert.equal(notice.kind, "error");
  assert.equal(notice.focusable, false, "the cursor never lands on a message row");
  assert.equal(notice.id, "error:bad");
});

test("activate opens the cursor's file and toggles its folder; nothing with no cursor", () => {
  assert.deepEqual(activate(NAV_ROWS, "z.txt"), { kind: "open", path: "z.txt" });
  assert.deepEqual(activate(NAV_ROWS, "a"), { kind: "toggle", path: "a" });
  assert.equal(activate(NAV_ROWS, null), null);
  assert.equal(activate(NAV_ROWS, "nowhere"), null);
});

test("reveal opens every folder above a path and puts the cursor on it, leaving the selection alone", () => {
  let s = reduce(initialState(), { type: "loaded", path: ROOT, entries: [{ name: "a", path: "a", dir: true, kind: "dir" }], truncated: false, root: "/r" });
  s = reduce(s, { type: "select", path: "a" });
  s = reduce(s, { type: "reveal", path: "a/b/c/deep.txt" });
  assert.deepEqual(s.open, { a: true, "a/b": true, "a/b/c": true });
  assert.equal(s.cursor, "a/b/c/deep.txt");
  assert.deepEqual(s.selection, ["a"], "a reveal moves the cursor, not the selection");
  assert.deepEqual(pendingLoads(s), ["a"], "the loads follow the open spine one listing at a time");
});

test("the list's own rows leave the root's notice to the panel frame, and keep a nested one", () => {
  // An empty root: the frame says "empty"; the list has nothing to draw.
  assert.deepEqual(bodyRows(loaded({ [ROOT]: [] })), []);
  // A root still listing, likewise.
  assert.deepEqual(bodyRows(reduce(initialState(), { type: "loading", path: ROOT })), []);
  // A root that failed: the frame shows the error and the retry.
  assert.deepEqual(bodyRows(reduce(initialState(), { type: "failed", path: ROOT, error: "denied" })), []);
  // An empty *nested* folder keeps its "empty" row — it sits in place of children.
  const nested = loaded({ [ROOT]: [entry("a", { dir: true })], a: [] }, ["a"]);
  assert.deepEqual(
    bodyRows(nested).map((r) => r.id),
    ["a", "empty:a"],
  );
  // A truncated root keeps its "…and more" row: that is about the listing, not the root.
  let t = reduce(initialState(), { type: "loaded", path: ROOT, entries: [{ name: "x", path: "x", dir: false, kind: "file" }], truncated: true, root: "/r" });
  assert.deepEqual(
    bodyRows(t).map((r) => r.id),
    ["x", "truncated:"],
  );
});

test("a new file on an empty root is one draft row — the list always has something to mount for it", () => {
  const rows = withDraft(bodyRows(loaded({ [ROOT]: [] })), { dir: ROOT, kind: "file" });
  assert.equal(rows.length, 1);
  assert.equal(rows[0].kind, "draft");
  assert.equal(rows[0].id, "__draft__");
  assert.equal(rows[0].depth, 0);
  assert.equal(rows[0].focusable, false, "the cursor stays off the draft; its field has the keyboard");
});

// --- glyphs ----------------------------------------------------------------

/**
 * The kind→glyph map, read out of the source rather than restated.
 *
 * `FILE_KIND_ICON` is typed as a total `Record<FileEntryKind, LucideIcon>`, so
 * TypeScript already refuses a missing variant — but only while it stays a
 * total Record. Someone widening the key type to `string` to unblock
 * something would silently reintroduce the fallback-to-blank this map exists
 * to remove, and the compiler would say nothing. Reading both files keeps the
 * check independent of how the map happens to be typed today.
 */
function fileKindVariants() {
  const ts = readFileSync(join(HERE, "..", "types.gen.ts"), "utf8");
  const decl = ts.match(/export type FileEntryKind =([^;]+);/);
  assert.ok(decl, "types.gen.ts no longer declares FileEntryKind");
  return [...decl[1].matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
}

function mappedKinds() {
  const ts = readFileSync(join(HERE, "icons.ts"), "utf8");
  const map = ts.match(/FILE_KIND_ICON: Record<FileEntryKind, LucideIcon> = \{([^}]+)\}/);
  assert.ok(map, "icons.ts no longer declares FILE_KIND_ICON as a total Record");
  return [...map[1].matchAll(/^\s*([A-Za-z_]+):/gm)].map((m) => m[1]);
}

test("every FileEntryKind the node can send has a glyph", () => {
  const variants = fileKindVariants();
  // Seven since the re-architecture: dir, file, work, note, artifact,
  // journal, state — a project and a workstream are roots, not entries.
  assert.ok(variants.length >= 7, `only found ${variants.length} kinds — the parse is wrong`);
  const mapped = new Set(mappedKinds());
  const missing = variants.filter((k) => !mapped.has(k));
  assert.deepEqual(missing, [], `FILE_KIND_ICON is missing ${missing.join(", ")}`);
});

test("the glyph map invents no kinds the node cannot send", () => {
  const variants = new Set(fileKindVariants());
  const extra = mappedKinds().filter((k) => !variants.has(k));
  assert.deepEqual(extra, [], `FILE_KIND_ICON maps ${extra.join(", ")}, which is not a kind`);
});

test("a domain concept keeps the one glyph it already has elsewhere", () => {
  // Two glyphs for a work item's folder would say the tree's work and the
  // goal screen's work item are different things.
  const ts = readFileSync(join(HERE, "icons.ts"), "utf8");
  for (const [kind, icon] of [
    ["work", "ICON.workItem"],
    ["note", "ICON.note"],
    ["journal", "ICON.journal"],
  ]) {
    assert.match(
      ts,
      new RegExp(`${kind}:\\s*${icon.replace(".", "\\.")},`),
      `FILE_KIND_ICON.${kind} should reuse ${icon}`,
    );
  }
});

test("a refresh that re-pends only the root is a different load key from nothing pending", async () => {
  const { loadKey, pendingLoads, reduce, ROOT } = await import("./fileTreeModel.mjs");
  assert.notEqual(loadKey([]), loadKey([ROOT]), "the root is the empty string; the count tells them apart");
  assert.notEqual(loadKey(["a b"]), loadKey(["a", "b"]));
  // A ready tree pends nothing; invalidating it pends exactly the root.
  let state = reduce(reduce(undefined, { type: "reset" }), { type: "loaded", path: ROOT, entries: [], truncated: false, root: "/r" });
  assert.deepEqual(pendingLoads(state), []);
  state = reduce(state, { type: "invalidate" });
  assert.deepEqual(pendingLoads(state), [ROOT]);
  assert.notEqual(loadKey(pendingLoads(state)), loadKey([]));
});

test("a watcher frame refreshes the listings it touched, and only the loaded ones", async () => {
  const { dirsToRefresh, pendingLoads, reduce, ROOT } = await import("./fileTreeModel.mjs");
  assert.deepEqual(dirsToRefresh({ kind: "created", path: "src/new.rs" }), ["src"]);
  assert.deepEqual(dirsToRefresh({ kind: "renamed", path: "docs/b.md", from: "src/a.md" }), ["docs", "src"]);
  assert.deepEqual(dirsToRefresh({ kind: "renamed", path: "src/b.md", from: "src/a.md" }), ["src"], "one folder, once");
  assert.deepEqual(dirsToRefresh({ kind: "modified", path: "README.md" }), [ROOT]);
  assert.deepEqual(dirsToRefresh({ kind: "rescan", path: "" }), [ROOT]);
  let state = reduce(reduce(undefined, { type: "reset" }), { type: "loaded", path: ROOT, entries: [{ path: "src", name: "src", dir: true, kind: "dir" }], truncated: false, root: "/r" });
  state = reduce(state, { type: "expand", path: "src" });
  state = reduce(state, { type: "loaded", path: "src", entries: [], truncated: false });
  assert.deepEqual(pendingLoads(state), []);
  const untouched = reduce(state, { type: "refresh_dirs", paths: ["docs"] });
  assert.equal(untouched, state, "a folder nobody loaded is left alone");
  const refreshed = reduce(state, { type: "refresh_dirs", paths: ["src"] });
  assert.deepEqual(pendingLoads(refreshed), ["src"], "only the touched listing re-pends");
  assert.equal(refreshed.dirs.src.entries.length, 0, "and its rows stay until the new listing lands");
});

// ---- the selection --------------------------------------------------------

/** Two folders and two files at the root, `src` open with two files inside. */
function selectable() {
  return loaded(
    {
      [ROOT]: [entry("docs", { dir: true }), entry("src", { dir: true }), entry("a.txt"), entry("z.txt")],
      src: [entry("src/lib.rs"), entry("src/main.rs")],
    },
    ["src"],
  );
}

test("a plain click or arrow selects one row; Cmd toggles; Shift takes the range from the anchor over the visible rows", () => {
  let s = reduce(selectable(), { type: "select", path: "docs" });
  assert.deepEqual([s.cursor, s.anchor, s.selection], ["docs", "docs", ["docs"]]);
  s = reduce(s, { type: "select_toggle", path: "a.txt" });
  assert.deepEqual(s.selection, ["docs", "a.txt"]);
  assert.equal(s.anchor, "a.txt", "the anchor follows a toggle");
  s = reduce(s, { type: "select_toggle", path: "docs" });
  assert.deepEqual(s.selection, ["a.txt"], "toggled out");
  s = reduce(s, { type: "select_range", path: "src/lib.rs" });
  assert.deepEqual(s.selection, ["src/lib.rs", "src/main.rs", "a.txt"], "from the anchor up to the row, in display order");
  assert.equal(s.cursor, "src/lib.rs");
  s = reduce(s, { type: "cursor", path: "z.txt", extend: true });
  assert.deepEqual(s.selection, ["a.txt", "z.txt"], "Shift+arrow re-anchors the range");
  s = reduce(s, { type: "cursor", path: "src" });
  assert.deepEqual([s.selection, s.anchor], [["src"], "src"], "a plain arrow collapses the selection");
  s = reduce(s, { type: "select_range", path: "docs" });
  assert.deepEqual(s.selection, ["docs", "src"]);
  assert.deepEqual(reduce(initialState(), { type: "select_range", path: "x" }).selection, [], "a range to a row that is not there is nothing");
  assert.equal(reduce(s, { type: "select_range", path: "nowhere" }), s, "with an anchor too: the same state, not a range to nothing");
  const folded = reduce(reduce(selectable(), { type: "select", path: "src/lib.rs" }), { type: "collapse", path: "src" });
  assert.deepEqual(reduce(folded, { type: "select_range", path: "a.txt" }).selection, ["a.txt"], "an anchor that folded away starts no range: just here");
  assert.equal(reduce(folded, { type: "select_range", path: "src/main.rs" }), folded, "a row hidden inside the folded folder is not there");
});

test("select all takes every visible row and clear takes none; a folded folder counts as one row", () => {
  let s = reduce(selectable(), { type: "select_all" });
  assert.deepEqual(s.selection, ["docs", "src", "src/lib.rs", "src/main.rs", "a.txt", "z.txt"]);
  assert.equal(s.anchor, "docs");
  s = reduce(s, { type: "collapse", path: "src" });
  assert.deepEqual(s.selection, ["docs", "src", "a.txt", "z.txt"], "what folded away left the selection");
  s = reduce(s, { type: "select_clear" });
  assert.deepEqual([s.selection, s.anchor], [[], null]);
  assert.equal(reduce(s, { type: "select_clear" }), s, "clearing nothing is the same state");
});

test("a listing that changes under the selection prunes it: a reload without the row, a failure, a collapse-all", () => {
  let s = reduce(selectable(), { type: "select_all" });
  s = reduce(s, { type: "loaded", path: "src", entries: [entry("src/main.rs")], truncated: false });
  assert.deepEqual(s.selection, ["docs", "src", "src/main.rs", "a.txt", "z.txt"], "lib.rs is gone from disk and from the selection");
  s = reduce(s, { type: "failed", path: "src", error: "nope" });
  assert.deepEqual(s.selection, ["docs", "src", "a.txt", "z.txt"]);
  s = reduce(s, { type: "select", path: "src/x" });
  s = reduce(s, { type: "collapse_all" });
  assert.deepEqual(s.open, {});
  assert.deepEqual([s.selection, s.anchor], [[], null], "a row nobody can see is not selected");
});

test("targets are the selection when asked at a selected row, the row alone otherwise, never a child of a chosen folder", () => {
  let s = reduce(selectable(), { type: "select", path: "src" });
  s = reduce(s, { type: "select_toggle", path: "src/main.rs" });
  s = reduce(s, { type: "select_toggle", path: "a.txt" });
  const rows = visibleRows(s);
  assert.deepEqual(targetsOf(rows, s.selection, "a.txt"), [
    { path: "src", dir: true },
    { path: "a.txt", dir: false },
  ]);
  assert.deepEqual(targetsOf(rows, s.selection, "z.txt"), [{ path: "z.txt", dir: false }], "a row outside the selection acts alone");
  assert.deepEqual(targetsOf(rows, s.selection, null), []);
  assert.deepEqual(targetsOf(rows, s.selection, "nowhere"), []);
  assert.deepEqual(rangeBetween(rows, "z.txt", "src/main.rs"), ["src/main.rs", "a.txt", "z.txt"], "either direction, inclusive");
  assert.deepEqual(rangeBetween(rows, null, "a.txt"), ["a.txt"]);
});

test("shown is the inline preview and moves apart from the selection", () => {
  let s = reduce(selectable(), { type: "show", path: "a.txt" });
  assert.equal(s.shown, "a.txt");
  assert.deepEqual(s.selection, []);
  s = reduce(s, { type: "select", path: "z.txt" });
  assert.equal(s.shown, "a.txt", "selecting another row does not change what is shown");
  s = reduce(s, { type: "hide" });
  assert.equal(s.shown, null);
  assert.equal(reduce(s, { type: "hide" }), s);
});

test("a tree handed its open folders starts unfolded as it stood, and lists only what can be seen", () => {
  const dir = (path) => ({ name: path.split("/").pop(), path, dir: true, kind: "dir" });
  let s = initialState(["src", "src/ui", "gone", "docs/deep"]);
  assert.deepEqual(openFolderPaths(s.open), ["src", "src/ui", "gone", "docs/deep"]);
  assert.deepEqual(pendingLoads(s), [ROOT], "nothing under the root is asked for before the root answers");
  s = reduce(s, { type: "loaded", path: ROOT, entries: [dir("docs"), dir("src")], truncated: false, root: "/r" });
  assert.deepEqual(pendingLoads(s), ["src"], "a kept folder that is there is listed; one that is gone, or under a folded one, is not");
  s = reduce(s, { type: "loaded", path: "src", entries: [dir("src/ui")], truncated: false });
  assert.deepEqual(pendingLoads(s), ["src/ui"]);
  assert.deepEqual(visibleRows(s).filter((r) => r.kind === "entry").map((r) => [r.path, r.expanded]), [["docs", false], ["src", true], ["src/ui", true]]);
  assert.equal(s.cursor, null, "the cursor and the selection are the tree's own: they start over");
  assert.deepEqual(s.selection, []);
});

test("another root starts as that root was left unfolded", () => {
  const s = reduce(initialState(["src"]), { type: "reset", open: ["lib"] });
  assert.deepEqual(s, initialState(["lib"]));
  assert.deepEqual(reduce(s, { type: "reset" }), initialState(), "a root with nothing kept starts folded");
});

test("what is handed in that is no path is left out, and the open folders are handed out as they change", () => {
  assert.deepEqual(openFolderPaths(initialState(["src", "", 7, null, "src"]).open), ["src"]);
  assert.deepEqual(openFolderPaths(initialState(null).open), []);
  let s = reduce(initialState(), { type: "expand", path: "a" });
  s = reduce(s, { type: "expand", path: "a/b" });
  assert.deepEqual(openFolderPaths(s.open), ["a", "a/b"]);
  s = reduce(s, { type: "collapse", path: "a" });
  assert.deepEqual(openFolderPaths(s.open), ["a/b"], "a folded folder keeps what was unfolded under it");
  assert.deepEqual(openFolderPaths(reduce(s, { type: "collapse_all" }).open), []);
});

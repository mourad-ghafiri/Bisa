import assert from "node:assert/strict";
import { test } from "node:test";
import { initialState, reduce } from "../../ui/fileTreeModel.mjs";
import {
  FILTER_LIMIT,
  MEMORY_KEY,
  ROOT_FOLDER,
  ROOT_ID,
  TYPED_ID,
  ancestorsOf,
  choiceWords,
  filterProblem,
  filterRows,
  folderProblem,
  folderRows,
  foldersOf,
  idOfFolder,
  indexFolders,
  listedFolders,
  normalizeFolder,
  parseRemembered,
  rememberFolder,
  rememberedFolder,
  serverFor,
  stepCursor,
  submitWords,
} from "./serveFolderModel.mjs";

const dir = (path, extra = {}) => ({ path, name: path.split("/").at(-1), kind: "dir", dir: true, symlink: false, ignored: false, ...extra });
const file = (path) => ({ path, name: path.split("/").at(-1), kind: "file", dir: false, symlink: false, ignored: false });
const listed = (state, path, entries) => reduce(state, { type: "loaded", path, entries, truncated: false, root: "/checkout" });

test("a folder's row id is its path, and the root's — whose path is empty — is a name of its own", () => {
  assert.equal(idOfFolder(ROOT_FOLDER), ROOT_ID);
  assert.equal(idOfFolder("docs/build"), "docs/build");
  assert.notEqual(ROOT_ID, TYPED_ID, "the typed row is a row of its own");
});

test("the folders are every one the file paths name, each once, in path order", () => {
  assert.deepEqual(foldersOf(["src/ui/a.ts", "src/ui/b.ts", "src/main.ts", "README.md", "docs/build/index.html"]), ["docs", "docs/build", "src", "src/ui"]);
  assert.deepEqual(foldersOf([]), []);
  assert.deepEqual(foldersOf(null), []);
});

test("a folder with an index.html is known from the paths — the root's included", () => {
  const pages = indexFolders(["index.html", "docs/build/index.html", "src/index.html.bak", "site/page.html"]);
  assert.deepEqual([...pages].sort(), ["", "docs/build"]);
});

test("the tree is Root, then the folders as listed: no file, no chevron on a folder that holds none, an ignored one marked", () => {
  let state = listed(initialState(), "", [dir("src"), dir("dist", { ignored: true }), file("README.md")]);
  let rows = folderRows(state, "web-app");
  assert.deepEqual(
    rows.map((r) => [r.kind, r.id, r.depth, r.label, r.expandable]),
    [
      ["root", ROOT_ID, 0, "web-app", false],
      // `sortEntries` is the explorer's own order.
      ["folder", "dist", 1, "dist", true],
      ["folder", "src", 1, "src", true],
    ],
  );
  assert.equal(rows[1].ignored, true);

  // Opened: a listing in flight is said in place, then its folders nest under it.
  state = reduce(state, { type: "expand", path: "src" });
  state = reduce(state, { type: "loading", path: "src" });
  assert.deepEqual(folderRows(state, "web-app").map((r) => r.kind), ["root", "folder", "folder", "loading"]);
  state = listed(state, "src", [dir("src/ui"), file("src/main.ts")]);
  rows = folderRows(state, "web-app");
  assert.deepEqual(rows.map((r) => [r.id, r.depth]), [[ROOT_ID, 0], ["dist", 1], ["src", 1], ["src/ui", 2]]);

  // A folder listed and found to hold only files loses its chevron and draws nothing under it.
  state = reduce(state, { type: "expand", path: "src/ui" });
  state = listed(state, "src/ui", [file("src/ui/a.ts")]);
  const leaf = folderRows(state, "web-app").find((r) => r.id === "src/ui");
  assert.equal(leaf.expandable, false);
  assert.equal(leaf.expanded, false);
  assert.equal(folderRows(state, "web-app").length, 4, "no *empty* row for a folder of files");
  assert.deepEqual(listedFolders(state).sort(), ["dist", "src", "src/ui"]);
});

test("a listing that failed is one row in place of the folder's children, and no cursor lands on it", () => {
  let state = listed(initialState(), "", [dir("vault")]);
  state = reduce(state, { type: "expand", path: "vault" });
  state = reduce(state, { type: "failed", path: "vault", error: "permission denied" });
  const rows = folderRows(state, "web-app");
  assert.equal(rows.at(-1).kind, "error");
  assert.equal(rows.at(-1).error, "permission denied");
  assert.equal(rows.at(-1).focusable, false);
  assert.equal(stepCursor(rows, "vault", 1), "vault", "the last row a cursor can stand on");
});

test("the filter finds a folder by a few letters, offers Root by its word or the project's name, and a path as typed last", () => {
  const folders = ["docs", "docs/build", "src", "src/ui", "src/ui"];
  assert.deepEqual(filterRows("", folders, "web-app"), []);
  assert.deepEqual(filterRows("dbu", folders, "web-app").map((r) => [r.kind, r.folder]), [["match", "docs/build"], ["typed", "dbu"]]);
  assert.deepEqual(filterRows("ro", folders, "web-app")[0].id, ROOT_ID);
  assert.deepEqual(filterRows("web", folders, "web-app")[0].id, ROOT_ID);

  // A folder no build has made yet can still be named; one that exists is never offered twice.
  const made = filterRows("./out/site/", folders, "web-app");
  assert.deepEqual([made.at(-1).kind, made.at(-1).id, made.at(-1).folder], ["typed", TYPED_ID, "out/site"]);
  assert.equal(filterRows("docs/build", folders, "web-app").some((r) => r.kind === "typed"), false);
  assert.equal(filterRows("src/ui", folders, "web-app").filter((r) => r.folder === "src/ui").length, 1, "a folder known twice is listed once");

  // Above the checkout, or absolute: never offered, and the reason is said.
  for (const bad of ["../up", "/etc", "a/../../b", "C:\\Users"]) {
    assert.equal(filterRows(bad, folders, "web-app").some((r) => r.kind === "typed"), false, bad);
    assert.equal(filterProblem(bad), "A folder of the checkout: relative to it, never above it.");
  }
  assert.equal(filterProblem("zzz"), "No folder of the checkout matches.");
  assert.equal(filterRows("s", Array.from({ length: 200 }, (_, i) => `s${i}`), "web-app").filter((r) => r.kind === "match").length, FILTER_LIMIT);
});

test("the empty path is the root and no problem; a typed path is read as a folder", () => {
  assert.equal(folderProblem(""), null);
  assert.equal(folderProblem("docs/build"), null);
  assert.equal(normalizeFolder("  ./docs/build//  "), "docs/build");
  assert.equal(normalizeFolder("."), ROOT_FOLDER);
  assert.equal(normalizeFolder("docs\\build"), "docs/build");
});

test("the filter field's arrows walk the rows a cursor can stand on and stop at the ends", () => {
  const rows = [{ id: ROOT_ID }, { id: "a" }, { id: "loading:a", focusable: false }, { id: "b" }];
  assert.equal(stepCursor(rows, ROOT_ID, 1), "a");
  assert.equal(stepCursor(rows, "a", 1), "b", "a notice is stepped over");
  assert.equal(stepCursor(rows, "b", 1), "b");
  assert.equal(stepCursor(rows, ROOT_ID, -1), ROOT_ID);
  assert.equal(stepCursor(rows, "gone", 1), ROOT_ID, "a cursor on nothing starts at the top");
  assert.equal(stepCursor(rows, null, -1), "b");
  assert.equal(stepCursor([], null, 1), null);
});

test("a remembered folder is shown by opening its ancestors, outermost first", () => {
  assert.deepEqual(ancestorsOf("docs/build/site"), ["docs", "docs/build"]);
  assert.deepEqual(ancestorsOf("docs"), []);
  assert.deepEqual(ancestorsOf(ROOT_FOLDER), []);
});

test("a folder already served opens its server instead of asking the node for a second one", () => {
  const servers = [
    { owner: { kind: "workstream", workstream: "w1", folder: "" }, port: 4173 },
    { owner: { kind: "workstream", workstream: "w1", folder: "docs/build" }, port: 4174 },
    { owner: { kind: "artifact", sha256: "x", name: "page.html" }, port: 4175 },
  ];
  assert.equal(serverFor(servers, ROOT_FOLDER).port, 4173);
  assert.equal(serverFor(servers, "docs/build").port, 4174);
  assert.equal(serverFor(servers, "src"), null);
  assert.equal(serverFor(null, "src"), null);
  assert.equal(submitWords(serverFor(servers, "docs/build")), "Open :4174");
  assert.equal(submitWords(null), "Serve and open");
  assert.equal(choiceWords(ROOT_FOLDER, null), "Serves the whole checkout on a port of this machine — index.html for a directory, never a dotfile.");
  assert.equal(choiceWords("docs/build", null), "Serves `docs/build/` on a port of this machine — index.html for a directory, never a dotfile.");
  assert.equal(choiceWords("docs/build", { port: 4174 }), "Already serving `docs/build/` on :4174 — opens it in a browser tab.");
});

test("the folder last served is remembered per checkout, newest last and capped; anything else reads as the root", () => {
  assert.equal(MEMORY_KEY, "bisa.ide.serve.folder");
  let kept = rememberFolder({}, "w1", "docs/build/");
  kept = rememberFolder(kept, "w2", "");
  kept = rememberFolder(kept, "w1", "site");
  assert.deepEqual(Object.entries(kept), [["w2", ""], ["w1", "site"]]);
  assert.equal(rememberedFolder(kept, "w1"), "site");
  assert.equal(rememberedFolder(kept, "w3"), ROOT_FOLDER);
  assert.equal(rememberedFolder(null, "w1"), ROOT_FOLDER);
  assert.deepEqual(Object.keys(rememberFolder(kept, "w3", "a", 2)), ["w1", "w3"], "the oldest goes");

  assert.deepEqual(parseRemembered(JSON.stringify(kept)), kept);
  assert.deepEqual(parseRemembered(JSON.stringify({ w1: "../up", w2: 7, w3: "ok/" })), { w3: "ok" });
  for (const garbage of [null, "", "not json", "[1,2]", "7", '"text"']) assert.deepEqual(parseRemembered(garbage), {}, String(garbage));
});

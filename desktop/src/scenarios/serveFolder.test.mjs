/**
 * Scenario: a person serves a folder from the Browser button (ide/18).
 *
 * The caret has one *From folder…*; the picker opens on Root; a few letters
 * find `docs/build`; serving it is remembered, so ⌘⇧R — whose door the picker
 * is while nothing is up — opens on it next time, and once it is up the same
 * choice opens the server instead of starting a second one. A scenario steps
 * the models the way the components do — no DOM.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import { initialState, reduce } from "../ui/fileTreeModel.mjs";
import { runDoor } from "../views/_workbench/runCommandModel.mjs";
import { browserMenu } from "../views/_workbench/serversModel.mjs";
import {
  ROOT_FOLDER,
  ROOT_ID,
  ancestorsOf,
  choiceWords,
  filterRows,
  folderRows,
  foldersOf,
  idOfFolder,
  indexFolders,
  parseRemembered,
  rememberFolder,
  rememberedFolder,
  serverFor,
  stepCursor,
  submitWords,
} from "../views/_workbench/serveFolderModel.mjs";

const WID = "01JWORKSTREAM";
const PATHS = ["README.md", "src/main.ts", "docs/guide.md", "docs/build/index.html", "docs/build/assets/app.js"];
const dir = (path) => ({ path, name: path.split("/").at(-1), kind: "dir", dir: true, symlink: false, ignored: false });

test("from the caret to a served folder, and back to it with one chord", () => {
  // The caret: one item serves, and with nothing up it is ⌘⇧R's door.
  const door = runDoor({ run: null, servers: [] }).id;
  const menu = browserMenu({ checkout: true, servers: [], ports: [], tabs: [], annotate: null, busy: false, door });
  assert.deepEqual(menu.items.map((i) => i.label), ["New tab", "From folder…"]);
  assert.equal(menu.items[1].command, "run_project");

  // The picker opens on Root — nothing was served here before.
  let remembered = parseRemembered(null);
  let cursor = idOfFolder(rememberedFolder(remembered, WID));
  assert.equal(cursor, ROOT_ID);
  let tree = reduce(initialState(), { type: "loaded", path: "", entries: [dir("docs"), dir("src")], truncated: false, root: "/checkout" });
  let rows = folderRows(tree, "web-app");
  assert.deepEqual(rows.map((r) => r.id), [ROOT_ID, "docs", "src"]);
  assert.equal(submitWords(serverFor([], ROOT_FOLDER)), "Serve and open");
  assert.match(choiceWords(ROOT_FOLDER, null), /the whole checkout/);

  // A few letters find the build, and it is the one with a page to open.
  const found = filterRows("dbuild", foldersOf(PATHS), "web-app");
  assert.equal(found[0].folder, "docs/build");
  assert.ok(indexFolders(PATHS).has("docs/build"));
  cursor = stepCursor(found, null, 1);
  assert.equal(cursor, "docs/build");

  // Served: remembered for this checkout, and only this one.
  remembered = rememberFolder(remembered, WID, "docs/build");
  assert.equal(rememberedFolder(parseRemembered(JSON.stringify(remembered)), WID), "docs/build");
  assert.equal(rememberedFolder(remembered, "01JOTHER"), ROOT_FOLDER);

  // Next time the picker opens on it, shown: its ancestors open.
  for (const path of ancestorsOf("docs/build")) tree = reduce(tree, { type: "expand", path });
  tree = reduce(tree, { type: "loaded", path: "docs", entries: [dir("docs/build")], truncated: false, root: "/checkout" });
  rows = folderRows(tree, "web-app");
  assert.ok(rows.some((r) => r.id === idOfFolder("docs/build") && r.depth === 2));

  // It is up now: the same choice opens it, and ⌘⇧R's door is the server itself.
  const up = [{ id: "s1", owner: { kind: "workstream", workstream: WID, folder: "docs/build" }, port: 4173, url: "http://127.0.0.1:4173/", page: "http://127.0.0.1:4173/" }];
  assert.equal(submitWords(serverFor(up, "docs/build")), "Open :4173");
  assert.match(choiceWords("docs/build", serverFor(up, "docs/build")), /Already serving `docs\/build\/` on :4173/);
  assert.equal(runDoor({ run: null, servers: up }).id, "open:s1");
});

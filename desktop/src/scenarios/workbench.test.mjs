/**
 * A person's afternoon in the workbench, as the models see it: glance, keep,
 * split, move, close, reopen, and what a restart brings back. A scenario
 * steps the reducers the way the components do and asserts the facts a
 * screen would show after each step — no DOM, per the desktop's rule that a
 * wrong pixel is visible and a wrong fact is not (ide/03 § Tabs).
 *
 * Run with `node --test desktop/src/scenarios/workbench.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { leaves, leafOfTab, parseTree } from "../shell/paneTreeModel.mjs";
import {
  closeDocPane,
  closeTab,
  emptyWorkbench,
  isPreview,
  keepTab,
  moveDocToPane,
  openTab,
  panesFor,
  previewIds,
  reopenLastClosed,
  rootKey,
  splitDocPane,
  tabId,
  tabsFor,
} from "../views/_workbench/workbenchModel.mjs";

const KEY = rootKey("workstream", "01JSCENARIO");
const file = (path) => ({ kind: "file", path });
const ids = (state) => tabsFor(state, KEY).map(tabId);
const paneTabs = (state) => leaves(panesFor(state, KEY)).map((l) => l.tabs);
const activeIn = (state, id) => leafOfTab(panesFor(state, KEY), id).active;

test("a click through files leaves one preview behind, a double-click keeps it, and an edit keeps the next", () => {
  let s = emptyWorkbench();
  // Single clicks in the explorer: each glance replaces the last.
  s = openTab(s, KEY, file("README.md"), { preview: true });
  s = openTab(s, KEY, file("src/lib.rs"), { preview: true });
  s = openTab(s, KEY, file("src/main.rs"), { preview: true });
  assert.deepEqual(ids(s), ["file:src/main.rs"], "three glances, one tab");
  assert.deepEqual(previewIds(s, KEY), ["file:src/main.rs"]);
  assert.equal(reopenLastClosed(s, KEY).tab, null, "a replaced glance was never closed, so nothing reopens");

  // A double-click on the row keeps what is there.
  s = openTab(s, KEY, file("src/main.rs"));
  assert.ok(!isPreview(s, KEY, "file:src/main.rs"));
  assert.deepEqual(previewIds(s, KEY), [], "the slot is free again");

  // The next glance sits beside it; typing into it keeps it.
  s = openTab(s, KEY, file("Cargo.toml"), { preview: true });
  assert.deepEqual(ids(s), ["file:src/main.rs", "file:Cargo.toml"]);
  s = keepTab(s, KEY, "file:Cargo.toml");
  assert.deepEqual(previewIds(s, KEY), []);
  assert.deepEqual(ids(s), ["file:src/main.rs", "file:Cargo.toml"], "keeping moves nothing");
});

test("split right takes the active document across, a move brings one back, closing the pane merges what is left", () => {
  let s = emptyWorkbench();
  for (const p of ["a.rs", "b.rs", "c.rs"]) s = openTab(s, KEY, file(p));
  assert.equal(paneTabs(s).length, 1);
  assert.equal(activeIn(s, "file:c.rs"), "file:c.rs", "the last opened is shown");

  const split = splitDocPane(s, KEY, "row");
  const [left, right] = paneTabs(split);
  assert.deepEqual(left, ["file:a.rs", "file:b.rs"], "the pane it left keeps the other two");
  assert.deepEqual(right, ["file:c.rs"], "the active document went across");
  assert.equal(ids(split).length, 3, "no document was duplicated or lost");

  const rightLeaf = leafOfTab(panesFor(split, KEY), "file:c.rs");
  const moved = moveDocToPane(split, KEY, "file:a.rs", rightLeaf.id);
  assert.deepEqual(paneTabs(moved), [["file:b.rs"], ["file:c.rs", "file:a.rs"]]);
  assert.equal(activeIn(moved, "file:a.rs"), "file:a.rs", "a moved document is shown where it lands");

  const merged = closeDocPane(moved, KEY, rightLeaf.id);
  assert.equal(paneTabs(merged).length, 1, "the only pane left");
  assert.deepEqual([...paneTabs(merged)[0]].sort(), ["file:a.rs", "file:b.rs", "file:c.rs"], "its documents moved next door");
  assert.equal(ids(merged).length, 3);
});

test("a split never leaves a pane showing one document beside an empty choice", () => {
  let s = openTab(emptyWorkbench(), KEY, file("only.rs"));
  assert.equal(splitDocPane(s, KEY, "row"), s, "one document: nothing to split");
  s = openTab(s, KEY, file("second.rs"));
  assert.equal(splitDocPane(s, KEY, "row"), s, "two documents: one would be alone on each side");
  s = openTab(s, KEY, file("third.rs"));
  assert.notEqual(splitDocPane(s, KEY, "row"), s, "three: the split has two to leave behind");
});

test("close, then reopen: the last closed comes back where it was, and a preview that travelled through a split is kept", () => {
  let s = emptyWorkbench();
  for (const p of ["a.rs", "b.rs"]) s = openTab(s, KEY, file(p));
  s = openTab(s, KEY, file("glance.rs"), { preview: true });
  s = splitDocPane(s, KEY, "row");
  assert.ok(!isPreview(s, KEY, "file:glance.rs"), "a split takes the active tab across on purpose, so it is kept");
  assert.deepEqual(previewIds(s, KEY), []);

  const closed = closeTab(s, KEY, "file:b.rs");
  assert.deepEqual(ids(closed), ["file:a.rs", "file:glance.rs"]);
  const back = reopenLastClosed(closed, KEY);
  assert.deepEqual(back.tab, file("b.rs"));
  assert.deepEqual([...ids(back.state)].sort(), ["file:a.rs", "file:b.rs", "file:glance.rs"]);
  assert.equal(closeTab(back.state, KEY, "file:nope.rs"), back.state, "closing a tab that is not there changes nothing");
});

test("a restart brings back what you meant to keep: the layout is composed from kept tabs, and a pane forgets a preview it held", () => {
  let s = emptyWorkbench();
  s = openTab(s, KEY, file("kept.rs"));
  s = openTab(s, KEY, file("also.rs"));
  s = openTab(s, KEY, file("glance.rs"), { preview: true });
  assert.deepEqual(ids(s), ["file:kept.rs", "file:also.rs", "file:glance.rs"]);

  // What the layout saves: the tabs that are not previews, and the pane tree
  // as JSON — a preview is in neither (ide/03: previews are never saved).
  const kept = tabsFor(s, KEY).filter((t) => !isPreview(s, KEY, tabId(t)));
  const saved = JSON.parse(JSON.stringify({ tabs: kept, panes: panesFor(s, KEY) }));
  assert.deepEqual(saved.tabs.map(tabId), ["file:kept.rs", "file:also.rs"]);

  // Restored: the tree is read against the tabs that exist; the preview's id
  // is not one of them and leaves every pane.
  const tree = parseTree(saved.panes, saved.tabs.map(tabId));
  assert.ok(tree, "the saved tree still reads");
  const restoredTabs = leaves(tree).flatMap((l) => l.tabs);
  assert.deepEqual(restoredTabs.sort(), ["file:also.rs", "file:kept.rs"]);
  assert.ok(!restoredTabs.includes("file:glance.rs"));

  // And a restart restores into a fresh state the same way the store does:
  // each kept tab opened kept, the panes as read.
  let again = emptyWorkbench();
  for (const t of saved.tabs) again = openTab(again, KEY, t);
  assert.deepEqual(ids(again), ["file:kept.rs", "file:also.rs"]);
  assert.deepEqual(previewIds(again, KEY), [], "nothing restored is a preview");
});

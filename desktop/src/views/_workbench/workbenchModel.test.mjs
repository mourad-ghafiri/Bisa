/**
 * The rules in `workbenchModel.mjs`, one test per invariant.
 *
 * Run with `npm test` from `desktop/`.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { findLeaf, leafOfTab, leaves } from "../../shell/paneTreeModel.mjs";

import { MAX_RECENTLY_CLOSED, MAX_ROOTS, capRoots, heldRoots, activateDoc, closeAllTabs, closeDocPane, closeOtherTabs, closeSavedTabs, closeTab, closeTabsRight, emptyWorkbench, forgetRoot, isPinned, isPreview, keepTab, leafStrip, mergedTabs, moveDocToPane, moveTab, nextActiveAfterClose, nextUntitledSeq, openTab, openTabBeside, panesFor, parseTabId, previewIds, reconcileStrip, reopenLastClosed, replaceTab, retargetTabs, rootKey, savedTabs, splitDocPane, stripOrder, tabId, tabLabel, tabTitle, tabsFor, tabsRightOf, tabsUnder, togglePin, untitledName, tabIn } from "./workbenchModel.mjs";

/** The leaf a document sits in, through the model's own door. */
const paneOf = (state, key, id) => leafOfTab(panesFor(state, key), id);
/** The focused pane of a root: what the record says, else the first leaf. */
const focusedPaneOf = (state, key) => {
  const entry = state.roots.find((r) => r.key === key);
  return entry ? (findLeaf(entry.panes, entry.focusedPane)?.id ?? leaves(entry.panes)[0].id) : "d1";
};


const file = (path) => ({ kind: "file", path });
const KEY = rootKey("workstream", "01JPROJECT");

test("a root with nothing open has no tabs, and closing everything leaves none", () => {
  const empty = emptyWorkbench();
  assert.deepEqual(tabsFor(empty, KEY), []);
  let state = openTab(empty, KEY, file("src/main.rs"));
  assert.deepEqual(tabsFor(state, KEY), [file("src/main.rs")]);
  state = closeAllTabs(state, KEY);
  assert.deepEqual(tabsFor(state, KEY), []);
});

test("a tab id round-trips a path that contains a colon", () => {
  const tab = file("src/a:b.txt");
  assert.equal(tabId(tab), "file:src/a:b.txt");
  assert.deepEqual(parseTabId("file:src/a:b.txt"), tab);
  assert.equal(parseTabId("git"), null, "the Git views live in the right panel alone; there is no Git document");
  assert.deepEqual(parseTabId("diff"), { kind: "diff" });
  assert.equal(parseTabId("file:"), null);
  assert.equal(parseTabId("about"), null, "there is no About tab any more");
  const transcript = { kind: "transcript", session: "s-9f", title: "dev" };
  assert.equal(tabId(transcript), "transcript:s-9f");
  assert.deepEqual(parseTabId("transcript:s-9f"), { ...transcript, title: "" }, "a reload keeps the session; the title is read again from the roster");
  assert.equal(parseTabId("transcript:"), null);
  assert.equal(tabLabel(transcript), "dev");
  assert.equal(tabLabel({ ...transcript, title: "" }), "Transcript");
  assert.equal(tabTitle(transcript), "Transcript — dev");
  assert.equal(parseTabId("nonsense"), null);
  assert.equal(parseTabId(undefined), null);
});

test("a patch tab id carries its side before the path, and a commit tab its sha; both round-trip", () => {
  const staged = { kind: "patch", path: "src/a:b.txt", staged: true };
  const tree = { kind: "patch", path: "src/main.rs", staged: false };
  assert.equal(tabId(staged), "patch:staged:src/a:b.txt");
  assert.equal(tabId(tree), "patch:worktree:src/main.rs");
  assert.deepEqual(parseTabId("patch:staged:src/a:b.txt"), staged, "the path may hold colons: the side is read first, the path is the rest");
  assert.deepEqual(parseTabId("patch:worktree:src/main.rs"), tree);
  assert.equal(parseTabId("patch:index:src/main.rs"), null, "a side off the list is no patch");
  assert.equal(parseTabId("patch:staged:"), null);
  assert.equal(tabLabel(staged), "a:b.txt · staged");
  assert.equal(tabLabel(tree), "main.rs · tree");
  assert.equal(tabTitle(staged), "Patch — src/a:b.txt (staged)");
  assert.equal(tabTitle(tree), "Patch — src/main.rs (working tree)");
  const commit = { kind: "commit", sha: "deadbeefcafe0123456789abcdef0123456789ab" };
  assert.equal(tabId(commit), `commit:${commit.sha}`);
  assert.deepEqual(parseTabId(`commit:${commit.sha}`), commit);
  assert.equal(parseTabId("commit:not-a-sha"), null, "a commit tab names a hex id");
  assert.equal(tabLabel(commit), "deadbee", "the strip shows the short id");
  assert.equal(tabTitle(commit), `Commit ${commit.sha}`);
  let state = openTab(emptyWorkbench(), KEY, staged, { preview: true });
  assert.ok(isPreview(state, KEY, tabId(staged)), "a glance at a patch is a preview, replaced by the next");
  state = openTab(state, KEY, tree, { preview: true });
  assert.deepEqual(tabsFor(state, KEY), [tree]);
});

test("a terminal tab id round-trips and is never stored", () => {
  const tab = { kind: "terminal", key: "t3" };
  assert.equal(tabId(tab), "terminal:t3");
  assert.deepEqual(parseTabId("terminal:t3"), tab);
  assert.equal(parseTabId("terminal:nope"), null, "only a minted key is a terminal");
  const empty = emptyWorkbench();
  assert.equal(openTab(empty, KEY, tab), empty, "the terminal store owns sessions");
  assert.equal(tabLabel(tab), "Terminal t3");
  assert.equal(tabTitle(tab), "Terminal t3");
});

test("a browser tab id round-trips, is never stored, and rides the strip with the terminals", () => {
  const tab = { kind: "browser", key: "b2" };
  assert.equal(tabId(tab), "browser:b2");
  assert.deepEqual(parseTabId("browser:b2"), tab);
  assert.equal(parseTabId("browser:t2"), null, "only a minted browser key");
  const empty = emptyWorkbench();
  assert.equal(openTab(empty, KEY, tab), empty, "the browser store owns tabs");
  assert.equal(tabLabel(tab), "Browser b2");
  assert.equal(tabTitle(tab), "Browser b2");
  const docs = [file("a.md")];
  assert.deepEqual(mergedTabs(docs, [{ key: "t1" }], [], [{ key: "b1" }]).map(tabId), ["file:a.md", "terminal:t1", "browser:b1"]);
  assert.deepEqual(mergedTabs(docs, [{ key: "t1" }], ["browser:b1", "file:a.md"], [{ key: "b1" }]).map(tabId), ["browser:b1", "file:a.md", "terminal:t1"], "the root's order decides");
  let s = openTab(empty, KEY, file("a.md"));
  s = reconcileStrip(s, KEY, ["file:a.md", "browser:b1"]);
  assert.equal(replaceTab(s, KEY, "file:a.md", tab), s, "a browser tab never replaces a document");
});

test("the strip is one order, newest last — a file opened after a shell is after it, a harness after that is last", () => {
  const docs = [file("b.md"), file("a.md")];
  // With no order yet, what came first draws first: the documents, then the terminals as they came.
  assert.deepEqual(mergedTabs(docs, [{ key: "t2" }, { key: "t1" }]).map(tabId), ["file:b.md", "file:a.md", "terminal:t2", "terminal:t1"]);
  assert.deepEqual(mergedTabs([], []), []);
  assert.deepEqual(mergedTabs(docs, undefined), docs);
  // The root's order decides; an id it has not seen comes after, as it came.
  const strip = ["terminal:t1", "file:a.md", "terminal:t2"];
  assert.deepEqual(mergedTabs(docs, [{ key: "t2" }, { key: "t1" }], strip).map(tabId), ["terminal:t1", "file:a.md", "terminal:t2", "file:b.md"]);
  // The lifecycle: a shell, then a file, then a harness, then a file.
  let s = reconcileStrip(emptyWorkbench(), KEY, ["terminal:t1"]);
  s = openTab(s, KEY, file("a.rs"));
  s = reconcileStrip(s, KEY, mergedTabs(tabsFor(s, KEY), [{ key: "t1" }, { key: "t2" }], s.roots[0].strip).map(tabId));
  s = openTab(s, KEY, file("b.rs"));
  const drawn = mergedTabs(tabsFor(s, KEY), [{ key: "t1" }, { key: "t2" }], s.roots[0].strip).map(tabId);
  assert.deepEqual(drawn, ["terminal:t1", "file:a.rs", "terminal:t2", "file:b.rs"]);
  // Reconciling what was drawn changes nothing; a terminal that closed is forgotten.
  assert.equal(reconcileStrip(s, KEY, drawn), s);
  const gone = reconcileStrip(s, KEY, ["terminal:t1", "file:a.rs", "file:b.rs"]);
  assert.deepEqual(gone.roots[0].strip, ["terminal:t1", "file:a.rs", "file:b.rs"]);
  // Re-opening a document leaves its place; closing one drops it.
  assert.deepEqual(openTab(s, KEY, file("a.rs")).roots[0].strip, s.roots[0].strip);
  assert.deepEqual(closeTab(s, KEY, "file:a.rs").roots[0].strip, ["terminal:t1", "terminal:t2", "file:b.rs"]);
});

test("a pane's strip is its documents and, for the first pane, the terminals, in the root's order with pins first", () => {
  const strip = ["file:a.rs", "terminal:t1", "file:b.rs", "file:c.rs"];
  assert.deepEqual(leafStrip(strip, ["file:c.rs", "file:a.rs"], ["terminal:t1"], []), ["file:a.rs", "terminal:t1", "file:c.rs"]);
  assert.deepEqual(leafStrip(strip, ["file:c.rs", "file:a.rs"], [], []), ["file:a.rs", "file:c.rs"], "another pane draws no terminal");
  assert.deepEqual(leafStrip(strip, ["file:c.rs", "file:a.rs"], ["terminal:t1"], ["file:c.rs"]), ["file:c.rs", "file:a.rs", "terminal:t1"], "pinned first");
  assert.deepEqual(leafStrip(strip, ["file:z.rs"], ["terminal:t7"], []), ["file:z.rs", "terminal:t7"], "unseen ids after, as they came");
});

test("opening a document already open changes nothing and moves nothing", () => {
  let state = emptyWorkbench();
  state = openTab(state, KEY, file("a.md"));
  state = openTab(state, KEY, file("b.md"));
  const again = openTab(state, KEY, file("a.md"));
  assert.equal(again, state, "same object: the store notifies on identity");
  assert.deepEqual(tabsFor(again, KEY).map(tabId), ["file:a.md", "file:b.md"]);
});

test("closing the active tab yields the neighbour to the right, then the left, then nothing", () => {
  let state = emptyWorkbench();
  for (const p of ["a.md", "b.md", "c.md"]) state = openTab(state, KEY, file(p));
  const tabs = tabsFor(state, KEY);

  assert.equal(nextActiveAfterClose(tabs, "file:b.md", "file:b.md"), "file:c.md");
  assert.equal(nextActiveAfterClose(tabs, "file:c.md", "file:c.md"), "file:b.md", "then the left");
  // Closing something you are not looking at does not move you.
  assert.equal(nextActiveAfterClose(tabs, "file:a.md", "file:c.md"), "file:c.md");
  // The last tab leaves nothing active: the centre shows its landing.
  assert.equal(nextActiveAfterClose([file("a.md")], "file:a.md", "file:a.md"), null);
  // A terminal tab in the strip is a neighbour like any other.
  const withTerminal = mergedTabs([file("a.md")], [{ key: "t1" }]);
  assert.equal(nextActiveAfterClose(withTerminal, "file:a.md", "file:a.md"), "terminal:t1");
});

test("a root's tabs survive leaving it and coming back", () => {
  let state = emptyWorkbench();
  state = openTab(state, KEY, file("a.md"));
  const other = rootKey("workstream", "01JWORKSTREAM");
  state = openTab(state, other, file("z.md"));
  assert.deepEqual(tabsFor(state, KEY).map(tabId), ["file:a.md"]);
  assert.deepEqual(tabsFor(state, other).map(tabId), ["file:z.md"]);
});

test("only the coldest root is forgotten, and never the one being used", () => {
  let state = emptyWorkbench();
  for (let i = 0; i < MAX_ROOTS + 2; i++) {
    const k = rootKey("workstream", `p${i}`);
    state = openTab(state, k, file("x.md"));
  }
  assert.equal(state.roots.length, MAX_ROOTS);
  assert.equal(state.roots[0].key, rootKey("workstream", `p${MAX_ROOTS + 1}`), "newest first");
  assert.ok(!state.roots.some((r) => r.key === rootKey("workstream", "p0")), "the oldest went");
  // Reopening in an old root promotes it rather than evicting it.
  const promoted = openTab(state, rootKey("workstream", "p2"), file("y.md"));
  assert.equal(promoted.roots[0].key, rootKey("workstream", "p2"));
});

test("the cap never takes a root that holds unsaved work: what was typed and not saved is not memory to reclaim", () => {
  const key = (i) => rootKey("workstream", `p${i}`);
  // Text typed in the first root opened, and never saved; then one root after another.
  const unsaved = [`${key(0)}|file:notes.md`, `${key(0)}|untitled:1`, `${key(3)}|file:a.rs`];
  assert.deepEqual(heldRoots(unsaved), [key(0), key(3)], "the roots of the buffers, none twice");
  assert.deepEqual(heldRoots([]), []);
  assert.deepEqual(heldRoots(["no bar here", "|file:x"]), [], "a key that names no root holds none");
  let state = emptyWorkbench();
  for (let i = 0; i < MAX_ROOTS + 3; i++) state = openTab(state, key(i), file("x.md"), { held: heldRoots(unsaved) });
  const kept = state.roots.map((r) => r.key);
  assert.ok(kept.includes(key(0)), "the oldest root stays: its document was never saved");
  assert.ok(!kept.includes(key(1)) && !kept.includes(key(2)), "the saved ones past the cap go, coldest first");
  assert.equal(kept.length, MAX_ROOTS + 1, "the cap, and the one root it may not take");
  assert.deepEqual(tabsFor(state, key(0)).map(tabId), ["file:x.md"], "with its tabs");
  // Saved at last: the next root opened lets it go.
  state = openTab(state, key(99), file("x.md"), { held: [] });
  assert.equal(state.roots.length, MAX_ROOTS);
  assert.ok(!state.roots.some((r) => r.key === key(0)));
  // A shell's strip and a document opened beside are held to the same cap.
  let beside = emptyWorkbench();
  for (let i = 0; i < MAX_ROOTS + 1; i++) beside = openTabBeside(beside, key(i), file("x.md"), "row", [key(0)]);
  assert.ok(beside.roots.some((r) => r.key === key(0)));
  let strips = emptyWorkbench();
  for (let i = 0; i < MAX_ROOTS + 1; i++) strips = reconcileStrip(strips, key(i), ["terminal:t1"], [key(0)]);
  assert.ok(strips.roots.some((r) => r.key === key(0)));
  assert.equal(reconcileStrip(emptyWorkbench(), key(0), ["terminal:t1"]).roots.length, 1, "nobody holding anything is the plain cap");
  // The rule itself: under the cap nothing goes; past it, the held ones stay where they were.
  const roots = Array.from({ length: MAX_ROOTS + 2 }, (_, i) => ({ key: key(i) }));
  const under = roots.slice(0, MAX_ROOTS);
  assert.equal(capRoots(under, null), under, "at the cap nothing goes: the same list");
  assert.deepEqual(capRoots(roots, [key(MAX_ROOTS + 1)]).map((r) => r.key), [...roots.slice(0, MAX_ROOTS).map((r) => r.key), key(MAX_ROOTS + 1)]);
  assert.deepEqual(capRoots(roots, null).length, MAX_ROOTS);
  // The store hands every opening the roots that hold unsaved work.
  const store = readFileSync(new URL("./workbenchStore.ts", import.meta.url), "utf8");
  assert.ok(store.includes("return heldRoots(unsavedNow());"));
  assert.equal(store.split("held()").length - 1 >= 5, true, "every door that may push a root past the cap says who holds work");
});

test("nothing that changes nothing returns a new object", () => {
  const state = openTab(emptyWorkbench(), KEY, file("a.md"));
  assert.equal(closeTab(state, KEY, "file:nope.md"), state);
  assert.equal(closeTab(state, "other", "file:a.md"), state);
  assert.equal(closeAllTabs(state, "other"), state);
  assert.equal(forgetRoot(state, "other"), state);
  assert.equal(closeOtherTabs(state, KEY, "file:a.md"), state);
});

test("close others keeps the one you named", () => {
  let state = emptyWorkbench();
  for (const p of ["a.md", "b.md", "c.md"]) state = openTab(state, KEY, file(p));
  state = closeOtherTabs(state, KEY, "file:b.md");
  assert.deepEqual(tabsFor(state, KEY), [file("b.md")]);
  assert.deepEqual(tabsFor(forgetRoot(state, KEY), KEY), []);
});

test("a tab is labelled by its basename and titled by its whole path", () => {
  assert.equal(tabLabel(file("src/main.rs")), "main.rs");
  assert.equal(tabTitle(file("src/main.rs")), "src/main.rs");
  assert.equal(tabLabel({ kind: "diff" }), "Diff");
  assert.equal(tabLabel({ kind: "diff" }), "Diff");
  assert.equal(tabLabel(null), "");
});

test("a closed document is remembered, most recent first, deduplicated and capped; a terminal never is", () => {
  let s = openTab(emptyWorkbench(), KEY, file("a.rs"));
  s = openTab(s, KEY, file("b.rs"));
  s = openTab(s, KEY, file("c.rs"));
  s = closeTab(s, KEY, "file:a.rs");
  s = closeTab(s, KEY, "file:b.rs");
  assert.deepEqual(s.roots[0].recentlyClosed.map(tabId), ["file:b.rs", "file:a.rs"]);
  s = openTab(s, KEY, file("a.rs"));
  s = closeTab(s, KEY, "file:a.rs");
  assert.deepEqual(s.roots[0].recentlyClosed.map(tabId), ["file:a.rs", "file:b.rs"], "closed again: moved to the front, not doubled");
  let many = emptyWorkbench();
  for (let i = 0; i < MAX_RECENTLY_CLOSED + 3; i++) {
    many = openTab(many, KEY, file(`f${i}.rs`));
    many = closeTab(many, KEY, `file:f${i}.rs`);
  }
  assert.equal(many.roots[0].recentlyClosed.length, MAX_RECENTLY_CLOSED);
  assert.equal(tabId(many.roots[0].recentlyClosed[0]), `file:f${MAX_RECENTLY_CLOSED + 2}.rs`);
  assert.equal(closeTab(s, KEY, "terminal:t1"), s, "a terminal is not stored here, so closing one changes nothing");
});

test("close others and close all remember what they closed", () => {
  let s = openTab(emptyWorkbench(), KEY, file("a.rs"));
  s = openTab(s, KEY, file("b.rs"));
  s = openTab(s, KEY, file("c.rs"));
  const others = closeOtherTabs(s, KEY, "file:b.rs");
  assert.deepEqual(others.roots[0].recentlyClosed.map(tabId), ["file:a.rs", "file:c.rs"]);
  const all = closeAllTabs(s, KEY);
  assert.deepEqual(all.roots[0].recentlyClosed.map(tabId), ["file:a.rs", "file:b.rs", "file:c.rs"]);
  assert.deepEqual(all.roots[0].tabs, []);
});

test("reopening brings back the most recently closed document that is not open again, at the end of the strip", () => {
  let s = openTab(emptyWorkbench(), KEY, file("a.rs"));
  s = openTab(s, KEY, file("b.rs"));
  s = closeTab(s, KEY, "file:a.rs");
  s = closeTab(s, KEY, "file:b.rs");
  s = openTab(s, KEY, file("b.rs"));
  const first = reopenLastClosed(s, KEY);
  assert.deepEqual(first.tab, file("a.rs"), "b is open again, so a comes back");
  assert.deepEqual(first.state.roots[0].tabs.map(tabId), ["file:b.rs", "file:a.rs"]);
  assert.deepEqual(first.state.roots[0].recentlyClosed.map(tabId), ["file:b.rs"], "what came back is forgotten; b stays remembered");
  const second = reopenLastClosed(first.state, KEY);
  assert.equal(second.tab, null, "b is open: nothing usable remains");
  assert.equal(second.state, first.state, "and the state is untouched");
  const unknown = reopenLastClosed(emptyWorkbench(), rootKey("goal", "01G"));
  assert.equal(unknown.tab, null);
});

test("a rename on disk moves the open tabs at or under the path and keeps their place", () => {
  let s = openTab(emptyWorkbench(), KEY, file("src/a.rs"));
  s = openTab(s, KEY, file("src/b.rs"));
  s = openTab(s, KEY, file("docs/x.md"));
  const one = retargetTabs(s, KEY, "src/a.rs", "src/c.rs");
  assert.deepEqual(one.moved, [["src/a.rs", "src/c.rs"]]);
  assert.deepEqual(one.state.roots[0].tabs.map(tabId), ["file:src/c.rs", "file:src/b.rs", "file:docs/x.md"], "in place");
  const folder = retargetTabs(s, KEY, "src", "lib");
  assert.deepEqual(folder.moved, [["src/a.rs", "lib/a.rs"], ["src/b.rs", "lib/b.rs"]]);
  assert.deepEqual(folder.state.roots[0].tabs.map(tabId), ["file:lib/a.rs", "file:lib/b.rs", "file:docs/x.md"]);
  const none = retargetTabs(s, KEY, "nope.rs", "x.rs");
  assert.equal(none.state, s, "nothing to move: the same state");
  assert.deepEqual(none.moved, []);
  assert.deepEqual(tabsUnder(s.roots[0].tabs, "src").map(tabId), ["file:src/a.rs", "file:src/b.rs"]);
  assert.deepEqual(tabsUnder(s.roots[0].tabs, "src/a.rs").map(tabId), ["file:src/a.rs"]);
  assert.deepEqual(tabsUnder(s.roots[0].tabs, "s"), [], "a prefix is not a folder");
});

test("documents open into the focused pane; a split takes the active one across; closing a pane moves its tabs next door", () => {
  let s = openTab(emptyWorkbench(), KEY, file("a.rs"));
  s = openTab(s, KEY, file("b.rs"));
  assert.equal(panesFor(s, KEY).kind, "leaf", "one pane until somebody splits");
  assert.equal(splitDocPane(s, KEY, "row"), s, "no split when only one document would be left showing");
  s = openTab(s, KEY, file("c.rs"));
  s = activateDoc(s, KEY, "file:c.rs");
  const split = splitDocPane(s, KEY, "row");
  assert.equal(panesFor(split, KEY).kind, "split");
  assert.equal(paneOf(split, KEY, "file:c.rs").id, focusedPaneOf(split, KEY), "the active document moved into the new, focused pane");
  assert.equal(paneOf(split, KEY, "file:a.rs").id, "d1");
  const moved = moveDocToPane(split, KEY, "file:a.rs", focusedPaneOf(split, KEY));
  assert.equal(paneOf(moved, KEY, "file:a.rs").id, focusedPaneOf(split, KEY));
  const closed = closeDocPane(moved, KEY, focusedPaneOf(moved, KEY));
  assert.equal(panesFor(closed, KEY).kind, "leaf", "back to one pane");
  assert.deepEqual([...panesFor(closed, KEY).tabs].sort(), ["file:a.rs", "file:b.rs", "file:c.rs"], "nothing dropped");
  const gone = closeTab(split, KEY, "file:c.rs");
  assert.equal(paneOf(gone, KEY, "file:c.rs"), null, "a closed tab leaves the tree");
  assert.equal(panesFor(gone, KEY).kind, "leaf", "and an emptied pane folds away");
  assert.equal(panesFor(emptyWorkbench(), rootKey("goal", "x")).kind, "leaf", "a root nobody opened has one pane");
});

test("a pinned tab sits first, has its place kept, survives close-others and close-all, and follows a rename", () => {
  let s = openTab(emptyWorkbench(), KEY, file("a.rs"));
  s = openTab(s, KEY, file("b.rs"));
  s = openTab(s, KEY, file("c.rs"));
  s = togglePin(s, KEY, "file:b.rs");
  assert.ok(isPinned(s, KEY, "file:b.rs"));
  assert.deepEqual(stripOrder(["file:a.rs", "file:b.rs", "file:c.rs"], s.roots[0].pinned), ["file:b.rs", "file:a.rs", "file:c.rs"]);
  const others = closeOtherTabs(s, KEY, "file:c.rs");
  assert.deepEqual(others.roots[0].tabs.map(tabId), ["file:b.rs", "file:c.rs"], "the pinned one stays with the kept one");
  const all = closeAllTabs(s, KEY);
  assert.deepEqual(all.roots[0].tabs.map(tabId), ["file:b.rs"], "close all leaves the pins");
  assert.equal(closeAllTabs(all, KEY), all, "nothing left to close");
  const unpinned = togglePin(s, KEY, "file:b.rs");
  assert.ok(!isPinned(unpinned, KEY, "file:b.rs"));
  assert.equal(togglePin(s, KEY, "file:nope.rs"), s, "a tab that is not open cannot be pinned");
  const renamed = retargetTabs(s, KEY, "b.rs", "lib.rs");
  assert.ok(isPinned(renamed.state, KEY, "file:lib.rs"), "the pin follows the rename");
  assert.equal(paneOf(renamed.state, KEY, "file:lib.rs").id, "d1", "so does the pane");
  const gone = closeTab(s, KEY, "file:b.rs");
  assert.ok(!isPinned(gone, KEY, "file:b.rs"), "a closed tab is no longer pinned");
});

test("a tab drags along its strip, a pinned one only among the pinned, and the pane's order is what moves", () => {
  let s = openTab(emptyWorkbench(), KEY, file("a.rs"));
  s = openTab(s, KEY, file("b.rs"));
  s = openTab(s, KEY, file("c.rs"));
  const moved = moveTab(s, KEY, "file:c.rs", 0);
  assert.equal(moved.refused, null);
  assert.deepEqual(moved.state.roots[0].strip, ["file:c.rs", "file:a.rs", "file:b.rs"], "the strip's order is the root's");
  assert.deepEqual(leafStrip(moved.state.roots[0].strip, panesFor(moved.state, KEY).tabs, [], []), ["file:c.rs", "file:a.rs", "file:b.rs"]);
  assert.deepEqual(moved.state.roots[0].tabs.map(tabId), ["file:a.rs", "file:b.rs", "file:c.rs"], "the opened-in order is kept for recently-closed and the like");
  assert.equal(moveTab(s, KEY, "file:a.rs", 0).state, s, "same place: same state");
  assert.equal(moveTab(s, KEY, "file:nope.rs", 0).state, s, "a tab that is not open moves nothing");
  const pinned = togglePin(s, KEY, "file:c.rs");
  assert.deepEqual(leafStrip(pinned.roots[0].strip, panesFor(pinned, KEY).tabs, [], pinned.roots[0].pinned), ["file:c.rs", "file:a.rs", "file:b.rs"]);
  const across = moveTab(pinned, KEY, "file:a.rs", 0);
  assert.equal(across.state, pinned);
  assert.match(across.refused, /pinned tabs sit first/);
  const pinAway = moveTab(pinned, KEY, "file:c.rs", 2);
  assert.match(pinAway.refused, /pinned tab stays/);
  const within = moveTab(pinned, KEY, "file:b.rs", 1);
  assert.equal(within.refused, null);
  assert.deepEqual(leafStrip(within.state.roots[0].strip, panesFor(within.state, KEY).tabs, [], within.state.roots[0].pinned), ["file:c.rs", "file:b.rs", "file:a.rs"]);
});

test("a drag moves a terminal among documents and a document past a terminal — one run — and close-to-the-right leaves a shell alone", () => {
  let s = openTab(emptyWorkbench(), KEY, file("a.rs"));
  s = reconcileStrip(s, KEY, ["file:a.rs", "terminal:t1"]);
  s = openTab(s, KEY, file("b.rs"));
  const shown = leafStrip(s.roots[0].strip, panesFor(s, KEY).tabs, ["terminal:t1"], []);
  assert.deepEqual(shown, ["file:a.rs", "terminal:t1", "file:b.rs"]);
  // The shell to the end.
  const shellLast = moveTab(s, KEY, "terminal:t1", 2, shown);
  assert.equal(shellLast.refused, null);
  assert.deepEqual(shellLast.state.roots[0].strip, ["file:a.rs", "file:b.rs", "terminal:t1"]);
  // A document before the shell, from the strip it was dragged along.
  const docFirst = moveTab(shellLast.state, KEY, "file:b.rs", 0, ["file:a.rs", "file:b.rs", "terminal:t1"]);
  assert.deepEqual(docFirst.state.roots[0].strip, ["file:b.rs", "file:a.rs", "terminal:t1"]);
  // Pins still sit first, terminals included in the count.
  const pinned = togglePin(docFirst.state, KEY, "file:a.rs");
  const pinnedShown = leafStrip(pinned.roots[0].strip, panesFor(pinned, KEY).tabs, ["terminal:t1"], pinned.roots[0].pinned);
  assert.deepEqual(pinnedShown, ["file:a.rs", "file:b.rs", "terminal:t1"]);
  assert.match(moveTab(pinned, KEY, "terminal:t1", 0, pinnedShown).refused, /pinned tabs sit first/);
  // Close to the right of the first document closes the documents after it and leaves the shell.
  assert.deepEqual(tabsRightOf(s, KEY, "file:a.rs"), ["file:b.rs"]);
  assert.deepEqual(tabsFor(closeTabsRight(s, KEY, "file:a.rs"), KEY).map(tabId), ["file:a.rs"]);
  assert.deepEqual(closeTabsRight(s, KEY, "file:a.rs").roots[0].strip, ["file:a.rs", "terminal:t1"]);
});

test("close to the right closes the unpinned tabs after this one in its strip, and nothing in another pane", () => {
  let s = openTab(emptyWorkbench(), KEY, file("a.rs"));
  s = openTab(s, KEY, file("b.rs"));
  s = openTab(s, KEY, file("c.rs"));
  s = openTab(s, KEY, file("d.rs"));
  s = togglePin(s, KEY, "file:d.rs");
  assert.deepEqual(tabsRightOf(s, KEY, "file:a.rs"), ["file:b.rs", "file:c.rs"], "the pinned one sits first, so it is never to the right");
  const closed = closeTabsRight(s, KEY, "file:a.rs");
  assert.deepEqual(closed.roots[0].tabs.map(tabId), ["file:a.rs", "file:d.rs"]);
  assert.equal(closeTabsRight(s, KEY, "file:c.rs"), s, "nothing to the right: nothing to do");
  assert.deepEqual(tabsRightOf(s, KEY, "file:nope.rs"), []);
  s = activateDoc(s, KEY, "file:c.rs");
  const split = splitDocPane(s, KEY, "row");
  assert.deepEqual(tabsRightOf(split, KEY, "file:a.rs"), ["file:b.rs"], "c moved to the other pane, so it is not to the right any more");
});

// ---- preview tabs ------------------------------------------------------------

test("a preview takes its pane's one preview slot: the next one replaces it, unremembered; a kept tab is never replaced", () => {
  let state = openTab(emptyWorkbench(), KEY, file("kept.rs"));
  state = openTab(state, KEY, file("a.rs"), { preview: true });
  assert.ok(isPreview(state, KEY, "file:a.rs"));
  assert.deepEqual(previewIds(state, KEY), ["file:a.rs"]);
  state = openTab(state, KEY, file("b.rs"), { preview: true });
  assert.deepEqual(tabsFor(state, KEY).map(tabId), ["file:kept.rs", "file:b.rs"], "a.rs left when b.rs took the slot");
  assert.deepEqual(previewIds(state, KEY), ["file:b.rs"]);
  assert.equal(paneOf(state, KEY, "file:b.rs").active, "file:b.rs", "the pane shows the new preview");
  assert.deepEqual(reopenLastClosed(state, KEY).tab, null, "a replaced preview was never closed, so it is not remembered");
  const again = openTab(state, KEY, file("kept.rs"), { preview: true });
  assert.deepEqual(tabsFor(again, KEY).map(tabId), ["file:kept.rs", "file:b.rs"], "a kept tab glanced at again is shown where it is: nothing opens, nothing leaves");
  assert.ok(!isPreview(again, KEY, "file:kept.rs"), "and it stays kept — a glance never demotes");
  assert.deepEqual(previewIds(again, KEY), ["file:b.rs"], "the pane's preview slot is untouched");
  assert.equal(paneOf(again, KEY, "file:kept.rs").active, "file:kept.rs", "the glance shows it");
  assert.equal(openTab(again, KEY, file("kept.rs"), { preview: true }), again, "glancing at the tab already shown is the same state");
});

test("a preview is kept by each door: keep, a kept open, pin, a move to another pane, a drag along the strip, a split", () => {
  const withPreview = () => openTab(openTab(emptyWorkbench(), KEY, file("k.rs")), KEY, file("p.rs"), { preview: true });
  const kept = keepTab(withPreview(), KEY, "file:p.rs");
  assert.ok(!isPreview(kept, KEY, "file:p.rs"));
  assert.deepEqual(tabsFor(kept, KEY).map(tabId), ["file:k.rs", "file:p.rs"], "keeping moves nothing");
  assert.equal(keepTab(kept, KEY, "file:p.rs"), kept, "keeping a kept tab is the same state — an effect may call it every render");
  assert.equal(keepTab(kept, KEY, "file:nope.rs"), kept);
  assert.ok(!isPreview(openTab(withPreview(), KEY, file("p.rs")), KEY, "file:p.rs"), "opened again as kept: promoted, not duplicated");
  assert.ok(!isPreview(togglePin(withPreview(), KEY, "file:p.rs"), KEY, "file:p.rs"));
  assert.ok(!isPreview(moveTab(withPreview(), KEY, "file:p.rs", 0).state, KEY, "file:p.rs"));
  let split = openTab(withPreview(), KEY, file("c.rs"));
  split = openTab(split, KEY, file("p.rs"), { preview: true });
  split = splitDocPane(split, KEY, "row");
  assert.ok(!isPreview(split, KEY, "file:c.rs") && !isPreview(split, KEY, "file:p.rs"));
  let panes = openTab(openTab(openTab(emptyWorkbench(), KEY, file("a.rs")), KEY, file("b.rs")), KEY, file("c.rs"));
  panes = splitDocPane(panes, KEY, "row");
  const other = leaves(panesFor(panes, KEY)).find((l) => l.id !== focusedPaneOf(panes, KEY)).id;
  panes = openTab(panes, KEY, file("p.rs"), { preview: true });
  panes = moveDocToPane(panes, KEY, "file:p.rs", other);
  assert.ok(!isPreview(panes, KEY, "file:p.rs"), "moved by hand: kept");
});

test("each pane has its own preview, and a closed pane's preview is kept before it moves next door", () => {
  let state = openTab(openTab(openTab(emptyWorkbench(), KEY, file("a.rs")), KEY, file("b.rs")), KEY, file("c.rs"));
  state = splitDocPane(state, KEY, "row");
  const right = focusedPaneOf(state, KEY);
  const left = leaves(panesFor(state, KEY)).find((l) => l.id !== right).id;
  state = openTab(state, KEY, file("pr.rs"), { preview: true });
  state = { roots: state.roots.map((r) => (r.key === KEY ? { ...r, focusedPane: left } : r)) };
  state = openTab(state, KEY, file("pl.rs"), { preview: true });
  assert.deepEqual(previewIds(state, KEY).sort(), ["file:pl.rs", "file:pr.rs"], "one per pane");
  assert.equal(paneOf(state, KEY, "file:pr.rs").id, right);
  assert.equal(paneOf(state, KEY, "file:pl.rs").id, left);
  const merged = closeDocPane(state, KEY, right);
  assert.deepEqual(previewIds(merged, KEY), ["file:pl.rs"], "the closed pane's preview arrived next door as a kept tab");
  assert.ok(tabsFor(merged, KEY).some((t) => tabId(t) === "file:pr.rs"));
});

test("closing forgets a preview record, a rename follows it, and Close saved leaves the dirty and the pinned", () => {
  let state = openTab(openTab(emptyWorkbench(), KEY, file("k.rs")), KEY, file("p.rs"), { preview: true });
  assert.deepEqual(previewIds(closeTab(state, KEY, "file:p.rs"), KEY), []);
  assert.deepEqual(previewIds(closeAllTabs(state, KEY), KEY), []);
  const renamed = retargetTabs(state, KEY, "p.rs", "q.rs");
  assert.deepEqual(previewIds(renamed.state, KEY), ["file:q.rs"]);
  state = openTab(state, KEY, file("dirty.rs"));
  state = togglePin(state, KEY, "file:k.rs");
  assert.deepEqual(savedTabs(state, KEY, ["file:dirty.rs"]), ["file:p.rs"]);
  const closed = closeSavedTabs(state, KEY, ["file:dirty.rs"]);
  assert.deepEqual(tabsFor(closed, KEY).map(tabId), ["file:k.rs", "file:dirty.rs"]);
  assert.equal(closeSavedTabs(closed, KEY, ["file:dirty.rs"]), closed, "nothing left to close is the same state");
});

test("an untitled document has a numbered id that round-trips, a name, and never a preview", () => {
  const untitled = { kind: "untitled", seq: 2 };
  assert.equal(tabId(untitled), "untitled:2");
  assert.deepEqual(parseTabId("untitled:2"), untitled);
  assert.equal(parseTabId("untitled:0"), null, "numbering starts at one");
  assert.equal(parseTabId("untitled:two"), null);
  assert.equal(parseTabId("untitled:"), null);
  assert.equal(tabLabel(untitled), "Untitled-2");
  assert.equal(untitledName(1), "Untitled-1");
  assert.ok(tabTitle(untitled).startsWith("Untitled-2"), "the title says it is not saved yet");
  // Asked for as a preview, it is kept anyway: a preview is replaced by the
  // next glance, and this is a buffer somebody is about to type into.
  const state = openTab(emptyWorkbench(), KEY, untitled, { preview: true });
  assert.deepEqual(tabsFor(state, KEY), [untitled]);
  assert.ok(!isPreview(state, KEY, "untitled:2"));
});

test("the next untitled number is the lowest one free, so a closed one's number comes back", () => {
  assert.equal(nextUntitledSeq([]), 1);
  assert.equal(nextUntitledSeq(null), 1);
  assert.equal(nextUntitledSeq([file("a.rs")]), 1, "files hold no number");
  assert.equal(nextUntitledSeq([{ kind: "untitled", seq: 1 }, { kind: "untitled", seq: 2 }]), 3);
  assert.equal(nextUntitledSeq([{ kind: "untitled", seq: 1 }, { kind: "untitled", seq: 3 }]), 2, "the gap is filled");
  let state = openTab(emptyWorkbench(), KEY, { kind: "untitled", seq: 1 });
  state = openTab(state, KEY, { kind: "untitled", seq: 2 });
  state = closeTab(state, KEY, "untitled:1");
  assert.equal(nextUntitledSeq(tabsFor(state, KEY)), 1);
});

test("saving an untitled document replaces its tab in place: the position, the pane and the pin stay", () => {
  let state = openTab(emptyWorkbench(), KEY, file("a.rs"));
  state = openTab(state, KEY, { kind: "untitled", seq: 1 });
  state = openTab(state, KEY, file("b.rs"));
  state = togglePin(state, KEY, "untitled:1");
  state = splitDocPane(state, KEY, "row");
  state = activateDoc(state, KEY, "untitled:1");
  const pane = paneOf(state, KEY, "untitled:1").id;
  const named = replaceTab(state, KEY, "untitled:1", file("notes/todo.md"));
  assert.deepEqual(
    tabsFor(named, KEY).map(tabId),
    ["file:a.rs", "file:notes/todo.md", "file:b.rs"],
    "the same place in the strip",
  );
  assert.equal(paneOf(named, KEY, "file:notes/todo.md").id, pane, "the same pane");
  assert.equal(paneOf(named, KEY, "untitled:1"), null);
  assert.ok(isPinned(named, KEY, "file:notes/todo.md"), "the pin follows");
  assert.ok(!isPinned(named, KEY, "untitled:1"));
  assert.equal(findLeaf(panesFor(named, KEY), pane).active, "file:notes/todo.md", "the pane's active id is renamed, never dropped");
  assert.ok(!leaves(panesFor(named, KEY)).some((l) => l.tabs.includes("untitled:1")));
  // Nothing to replace, or the file already open: the same state.
  assert.equal(replaceTab(named, KEY, "untitled:1", file("x.rs")), named);
  assert.equal(replaceTab(state, KEY, "untitled:1", file("a.rs")), state, "a file already open is not opened twice");
  assert.equal(replaceTab(state, KEY, "untitled:1", { kind: "terminal", key: "t1" }), state);
});

test("a loose file's id is its absolute path, it round-trips, is labelled by its name, and is never a preview", () => {
  const loose = { kind: "loose", path: "/Users/me/notes/todo.md" };
  assert.equal(tabId(loose), "loose:/Users/me/notes/todo.md");
  assert.deepEqual(parseTabId("loose:/Users/me/notes/todo.md"), loose);
  assert.deepEqual(parseTabId("loose:/a:b/c.txt"), { kind: "loose", path: "/a:b/c.txt" }, "a colon in the path is the path's");
  assert.equal(parseTabId("loose:relative.txt"), null, "a loose file is named absolutely");
  assert.equal(parseTabId("loose:/"), null);
  assert.equal(parseTabId("loose:/folder/"), null, "a folder is not a file");
  assert.equal(tabLabel(loose), "todo.md");
  assert.ok(tabTitle(loose).startsWith("/Users/me/notes/todo.md"), "the title is the whole path");
  const state = openTab(emptyWorkbench(), KEY, loose, { preview: true });
  assert.deepEqual(tabsFor(state, KEY), [loose]);
  assert.ok(!isPreview(state, KEY, tabId(loose)), "dropped or picked on purpose: kept");
  // Saving elsewhere replaces the tab in place, as naming an untitled document does.
  const moved = replaceTab(state, KEY, tabId(loose), { kind: "loose", path: "/Users/me/todo.md" });
  assert.deepEqual(tabsFor(moved, KEY).map(tabId), ["loose:/Users/me/todo.md"]);
});

test("the shown tab is the stored one — the same object across renders — and an unlisted id is parsed", () => {
  const docs = [file("README.md"), { kind: "patch", path: "src/a.rs", staged: true }, { kind: "untitled", seq: 2 }];
  assert.equal(tabIn(docs, "file:README.md"), docs[0], "identity, not a copy: the editor keys its effects on it");
  assert.equal(tabIn(docs, "patch:staged:src/a.rs"), docs[1]);
  assert.equal(tabIn(docs, "untitled:2"), docs[2]);
  assert.deepEqual(tabIn(docs, "file:src/new.rs"), { kind: "file", path: "src/new.rs" }, "not adopted yet: parsed, so the first render still shows it");
  assert.deepEqual(tabIn(docs, "terminal:t3"), { kind: "terminal", key: "t3" }, "a terminal is the terminal store's, never in docs");
  assert.equal(tabIn(docs, null), null);
  assert.equal(tabIn(docs, "nonsense"), null);
  assert.deepEqual(tabIn([], "file:x"), { kind: "file", path: "x" }, "an id that parses but names nothing stored is still a file tab");
});

test("a device tab id round-trips and is stored like a document (ide/19)", () => {
  const device = { kind: "device", id: "AAAA-1111" };
  assert.equal(tabId(device), "device:AAAA-1111");
  assert.deepEqual(parseTabId("device:AAAA-1111"), device);
  assert.equal(parseTabId("device:"), null, "no id, no tab");
  assert.equal(parseTabId("device:a/b"), null, "a path is not a device");
  assert.equal(tabLabel(device), "Device AAAA-111");
  assert.ok(tabTitle(device).startsWith("Device AAAA-1111 —"));
  const state = openTab(emptyWorkbench(), KEY, device);
  assert.deepEqual(tabsFor(state, KEY), [device], "kept like a document, not a process the store never lists");
});

test("openTabBeside splits a single pane and puts the document in the new half; a split root or an empty one takes it like any document; an open one is only shown", () => {
  const device = { kind: "device", id: "AAAA-1111" };
  // Nothing open: like any document, into the one pane.
  let state = openTabBeside(emptyWorkbench(), KEY, device);
  assert.equal(leaves(panesFor(state, KEY)).length, 1);
  // One pane with the code in it: split, the device in the new half, focused.
  state = openTab(emptyWorkbench(), KEY, file("src/main.dart"));
  state = openTabBeside(state, KEY, device);
  const panes = leaves(panesFor(state, KEY));
  assert.equal(panes.length, 2, "split beside the code");
  assert.deepEqual(panes[0].tabs, ["file:src/main.dart"], "the code stays where it was");
  assert.deepEqual(panes[1].tabs, ["device:AAAA-1111"]);
  assert.equal(panes[1].active, "device:AAAA-1111");
  assert.equal(focusedPaneOf(state, KEY), panes[1].id);
  assert.deepEqual(tabsFor(state, KEY).map(tabId), ["file:src/main.dart", "device:AAAA-1111"]);
  // Already open: shown where it is, no third pane.
  const again = openTabBeside(state, KEY, device);
  assert.equal(leaves(panesFor(again, KEY)).length, 2);
  assert.equal(paneOf(again, KEY, "device:AAAA-1111").active, "device:AAAA-1111");
  // A root already split takes a second device into the focused pane.
  const second = openTabBeside(again, KEY, { kind: "device", id: "Pixel_8" });
  assert.equal(leaves(panesFor(second, KEY)).length, 2, "no third pane");
  assert.ok(paneOf(second, KEY, "device:Pixel_8"), "placed in a pane");
});

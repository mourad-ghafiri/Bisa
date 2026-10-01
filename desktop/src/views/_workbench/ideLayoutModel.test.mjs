import test from "node:test";
import assert from "node:assert/strict";
import { LAYOUT_VERSION, restoreLayout, sameLayout, serializeLayout } from "./ideLayoutModel.mjs";
import { leaves, singleLeaf } from "../../shell/paneTreeModel.mjs";

test("a layout round-trips with its panes and pins, and a terminal tab is never saved", () => {
  const tabs = [{ kind: "file", path: "src/main.rs" }, { kind: "diff" }, { kind: "terminal", key: "t1" }];
  const panes = { kind: "split", id: "ds2", dir: "row", ratio: 0.5, a: singleLeaf("d1", ["file:src/main.rs"], "file:src/main.rs"), b: singleLeaf("d3", ["diff"], "diff") };
  const saved = serializeLayout(tabs, "file:src/main.rs", panes, ["diff", "terminal:t1"]);
  assert.equal(saved.version, LAYOUT_VERSION);
  assert.deepEqual(saved.tabs, [{ kind: "file", path: "src/main.rs" }, { kind: "diff" }]);
  assert.deepEqual(saved.pinned, ["diff"], "a pin on a terminal is not a pin");
  const back = restoreLayout(saved);
  assert.deepEqual(back.tabs, saved.tabs);
  assert.equal(back.active, "file:src/main.rs");
  assert.equal(back.panes.kind, "split");
  assert.deepEqual(back.pinned, ["diff"]);
  // An active terminal is saved as nothing active.
  assert.equal(serializeLayout(tabs, "terminal:t1").active, null);
  assert.equal(restoreLayout(serializeLayout([], null)).active, null);
  assert.equal(serializeLayout([], null).panes.kind, "leaf", "no tree given: one pane");
});

test("garbage in a saved layout is dropped, never thrown; an older layout is refused", () => {
  assert.equal(restoreLayout(null), null);
  assert.equal(restoreLayout({ version: 2, tabs: [], active: null }), null, "no ladder");
  assert.equal(restoreLayout({ version: 3, tabs: [{ kind: "git" }], active: null, panes: null, pinned: [] }), null, "a v3 layout could name the Git document, which is gone: refused, not reinterpreted");
  assert.equal(restoreLayout({ version: 4, tabs: [], active: null, panes: null, pinned: [] }), null, "a v4 layout has no strip order: refused, never reinterpreted");
  const r = restoreLayout({
    version: LAYOUT_VERSION,
    tabs: [{ kind: "file", path: "a.rs" }, { kind: "about" }, { kind: "git" }, 7, { kind: "file" }],
    active: 3,
    panes: { kind: "leaf", id: "d1", tabs: ["file:a.rs", "file:nope.rs"], active: "file:nope.rs" },
    pinned: ["file:a.rs", "file:nope.rs", 4],
  });
  assert.deepEqual(r.tabs, [{ kind: "file", path: "a.rs" }], "about and git are not tabs any more");
  assert.equal(r.active, null);
  assert.deepEqual(leaves(r.panes).flatMap((l) => l.tabs), ["file:a.rs"], "a tab the tree names but the layout does not is dropped");
  assert.deepEqual(r.pinned, ["file:a.rs"]);
});

test("a tab the saved tree forgot lands in the first pane, so every open document is somewhere", () => {
  const r = restoreLayout({
    version: LAYOUT_VERSION,
    tabs: [{ kind: "file", path: "a.rs" }, { kind: "file", path: "b.rs" }],
    active: null,
    panes: { kind: "leaf", id: "d1", tabs: ["file:a.rs"], active: "file:a.rs" },
    pinned: [],
  });
  assert.deepEqual(leaves(r.panes)[0].tabs, ["file:a.rs", "file:b.rs"]);
  const none = restoreLayout({ version: LAYOUT_VERSION, tabs: [{ kind: "diff" }], active: null, panes: "nonsense", pinned: [] });
  assert.deepEqual(leaves(none.panes)[0].tabs, ["diff"], "no usable tree: one pane holding them all");
  assert.ok(sameLayout(serializeLayout([], null), serializeLayout([], null)));
});

test("a preview is not saved — not as a tab, not in its pane — and an active preview is saved as the kept tab its pane would show", () => {
  const tabs = [{ kind: "file", path: "a.rs" }, { kind: "file", path: "p.rs" }, { kind: "file", path: "b.rs" }];
  const panes = singleLeaf("d1", ["file:a.rs", "file:p.rs", "file:b.rs"], "file:p.rs");
  const saved = serializeLayout(tabs, "file:p.rs", panes, [], ["file:p.rs"]);
  assert.deepEqual(saved.tabs, [{ kind: "file", path: "a.rs" }, { kind: "file", path: "b.rs" }]);
  assert.deepEqual(leaves(saved.panes)[0].tabs, ["file:a.rs", "file:b.rs"]);
  assert.equal(saved.active, "file:b.rs", "the neighbour to the right, as a close would leave it");
  const only = serializeLayout([{ kind: "file", path: "p.rs" }], "file:p.rs", singleLeaf("d1", ["file:p.rs"], "file:p.rs"), [], ["file:p.rs"]);
  assert.deepEqual(only.tabs, []);
  assert.equal(only.active, null);
  assert.ok(sameLayout(serializeLayout(tabs, "file:a.rs", panes, [], ["file:p.rs"]), serializeLayout(tabs.filter((t) => t.path !== "p.rs"), "file:a.rs", singleLeaf("d1", ["file:a.rs", "file:b.rs"], "file:a.rs"))), "a click through previews saves the same bytes");
});

test("an untitled document is not saved — not as a tab, not in its pane — and an active one is saved as its kept neighbour", () => {
  const tabs = [{ kind: "file", path: "a.rs" }, { kind: "untitled", seq: 1 }, { kind: "file", path: "b.rs" }];
  const panes = singleLeaf("d1", ["file:a.rs", "untitled:1", "file:b.rs"], "untitled:1");
  const saved = serializeLayout(tabs, "untitled:1", panes, ["untitled:1"]);
  assert.deepEqual(saved.tabs, [{ kind: "file", path: "a.rs" }, { kind: "file", path: "b.rs" }]);
  assert.deepEqual(leaves(saved.panes)[0].tabs, ["file:a.rs", "file:b.rs"]);
  assert.equal(saved.active, "file:b.rs", "the neighbour to the right, as a close would leave it");
  assert.deepEqual(saved.pinned, [], "a pin on an untitled document is not saved either");
  const only = serializeLayout([{ kind: "untitled", seq: 1 }], "untitled:1", singleLeaf("d1", ["untitled:1"], "untitled:1"));
  assert.deepEqual(only.tabs, []);
  assert.equal(only.active, null);
  assert.ok(sameLayout(saved, serializeLayout(tabs.filter((t) => t.kind !== "untitled"), "file:b.rs", singleLeaf("d1", ["file:a.rs", "file:b.rs"], "file:b.rs"))), "typing into an untitled document saves the same bytes");
  assert.deepEqual(restoreLayout({ version: LAYOUT_VERSION, tabs: [{ kind: "untitled", seq: 1 }], active: null, panes: null, pinned: [] }).tabs, [], "a layout that names one is read as not naming it");
});

test("a loose file is saved and restored by its absolute path, like a root file", () => {
  const tabs = [{ kind: "file", path: "a.rs" }, { kind: "loose", path: "/Users/me/todo.md" }];
  const saved = serializeLayout(tabs, "loose:/Users/me/todo.md");
  assert.deepEqual(saved.tabs, [{ kind: "file", path: "a.rs" }, { kind: "loose", path: "/Users/me/todo.md" }]);
  assert.equal(saved.active, "loose:/Users/me/todo.md");
  const back = restoreLayout(saved);
  assert.deepEqual(back.tabs, saved.tabs);
  assert.deepEqual(leaves(back.panes)[0].tabs, ["file:a.rs", "loose:/Users/me/todo.md"]);
  assert.deepEqual(restoreLayout({ version: LAYOUT_VERSION, tabs: [{ kind: "loose", path: "todo.md" }], active: null, panes: null, pinned: [] }).tabs, [], "a loose path is absolute or it is not one");
});

test("the strip's order is saved with the layout — documents and terminals as opened — and comes back", () => {
  const tabs = [{ kind: "file", path: "a.rs" }, { kind: "file", path: "b.rs" }, { kind: "terminal", key: "t1" }];
  const strip = ["file:a.rs", "terminal:t1", "file:b.rs"];
  const saved = serializeLayout(tabs, "file:b.rs", null, [], [], strip);
  assert.deepEqual(saved.strip, ["file:a.rs", "terminal:t1", "file:b.rs"], "a terminal keeps its place: the terminal store restores it under the same key");
  assert.deepEqual(restoreLayout(saved).strip, ["file:a.rs", "terminal:t1", "file:b.rs"]);
  // A preview or an untitled document is not saved, so neither is its place; junk is dropped.
  const hidden = serializeLayout([{ kind: "file", path: "a.rs" }, { kind: "file", path: "p.rs" }, { kind: "untitled", seq: 1 }], null, null, [], ["file:p.rs"], ["untitled:1", "file:p.rs", "file:a.rs"]);
  assert.deepEqual(hidden.strip, ["file:a.rs"]);
  assert.deepEqual(restoreLayout({ ...saved, strip: ["file:nope.rs", 7, "terminal:t9", "file:a.rs"] }).strip, ["terminal:t9", "file:a.rs"]);
  assert.deepEqual(restoreLayout({ ...saved, strip: "no" }).strip, []);
});

test("a device tab is saved with its id and comes back beside the files (ide/19)", () => {
  const tabs = [{ kind: "file", path: "lib/main.dart" }, { kind: "device", id: "AAAA-1111" }, { kind: "untitled", seq: 1 }];
  const saved = serializeLayout(tabs, "device:AAAA-1111");
  assert.deepEqual(saved.tabs, [{ kind: "file", path: "lib/main.dart" }, { kind: "device", id: "AAAA-1111" }], "an untitled document is left out of the layout");
  const back = restoreLayout(JSON.parse(JSON.stringify(saved)));
  assert.deepEqual(back.tabs, [{ kind: "file", path: "lib/main.dart" }, { kind: "device", id: "AAAA-1111" }], "the untitled buffer is not saved; the device is");
  assert.equal(back.active, "device:AAAA-1111");
  const bad = restoreLayout({ ...saved, tabs: [{ kind: "device", id: "" }, { kind: "device", id: "a/b" }] });
  assert.deepEqual(bad.tabs, [], "a device with no id or a path is dropped");
});

/**
 * The typed drag payloads. Run with `node --test desktop/src/ui/dnd/dragData.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { docTabDrag, dragCount, dragGlyph, dragType, hunkDrag, isDragOf, navRowDrag, pathDrag, railRowDrag, terminalTabDrag, workstreamCardDrag } from "./dragData.mjs";

test("every payload names its type and carries a label for the drag ghost", () => {
  assert.deepEqual(pathDrag({ scope: "workstream", id: "w1", path: "src/main.rs", dir: false }), {
    type: "path",
    scope: "workstream",
    id: "w1",
    path: "src/main.rs",
    dir: false,
    paths: ["src/main.rs"],
    label: "main.rs",
  });
  assert.equal(pathDrag({ scope: "workstream", id: "w1", path: "src", dir: true }).label, "src");
  const many = pathDrag({ scope: "workstream", id: "w1", path: "src", dir: true, paths: ["a.txt", "src", "b.txt"] });
  assert.deepEqual(many.paths, ["src", "a.txt", "b.txt"], "the row under the pointer first, once");
  assert.equal(many.label, "3 items");
  assert.equal(hunkDrag({ path: "a/b.ts", staged: true, text: "@@", id: "a/b.ts@3" }).label, "b.ts · hunk");
  assert.deepEqual(docTabDrag("file:src/x.ts", "leaf1", "x.ts"), { type: "doc-tab", id: "file:src/x.ts", pane: "leaf1", label: "x.ts" });
  assert.deepEqual(terminalTabDrag("t:3", "leaf2", "shell"), { type: "terminal-tab", key: "t:3", pane: "leaf2", label: "shell" });
  assert.deepEqual(railRowDrag("project", "p1", "", "Bisa"), { type: "rail-row", kind: "project", id: "p1", ctx: "", label: "Bisa" });
  assert.deepEqual(workstreamCardDrag("w1", "todo", "Cart total"), { type: "workstream-card", id: "w1", column: "todo", label: "Cart total" });
  assert.ok(isDragOf(workstreamCardDrag("w1", "todo", "x"), "workstream-card"));
});

test("a payload is recognised by its type and nothing else is", () => {
  const path = pathDrag({ scope: "goal", id: "g", path: "x", dir: false });
  assert.equal(dragType(path), "path");
  assert.ok(isDragOf(path, "path"));
  assert.ok(isDragOf(path, "hunk", "path"), "any of several types");
  assert.equal(isDragOf(path, "hunk"), false);
  assert.equal(dragType(null), null);
  assert.equal(dragType("a string"), null);
  assert.equal(dragType({ type: 3 }), null);
  assert.equal(isDragOf(undefined, "path"), false);
});

test("the ghost wears a glyph named for the payload, and a count for a selection", () => {
  assert.equal(dragGlyph(railRowDrag("group", "Shop", "", "Shop")), "folder");
  assert.equal(dragGlyph(railRowDrag("project", "p1", "ungrouped", "Alpha")), "project");
  assert.equal(dragGlyph(railRowDrag("workstream", "w1", "p1", "feat")), "workstream");
  assert.equal(dragGlyph(railRowDrag("terminal", "t1", "w1", "zsh")), "shell");
  assert.equal(dragGlyph(railRowDrag("agent", "a", "w1", "x")), null, "a kind with no picture");
  assert.equal(dragGlyph(pathDrag({ scope: "workstream", id: "w", path: "src", dir: true })), "folder");
  assert.equal(dragGlyph(pathDrag({ scope: "workstream", id: "w", path: "a.rs", dir: false })), "file");
  assert.equal(dragGlyph(docTabDrag("file:a", "l", "a")), "document");
  assert.equal(dragGlyph(hunkDrag({ path: "a", staged: false, text: "", id: "a@1" })), "document");
  assert.equal(dragGlyph(terminalTabDrag("t", "l", "zsh")), "shell");
  assert.equal(dragGlyph(workstreamCardDrag("w", "todo", "x")), "workstream");
  assert.equal(dragGlyph(navRowDrag("goals", "Goals", "goal")), "goal", "a destination wears its own glyph");
  assert.equal(dragGlyph(null), null);
  assert.equal(dragCount(pathDrag({ scope: "w", id: "w", path: "src", dir: true, paths: ["a", "b", "src"] })), 3);
  assert.equal(dragCount(pathDrag({ scope: "w", id: "w", path: "src", dir: true })), 1);
  assert.equal(dragCount(docTabDrag("file:a", "l", "a")), 1);
  assert.equal(dragCount(undefined), 1);
});

test("a sidebar destination carries its key, its name and its glyph", () => {
  assert.deepEqual(navRowDrag("pulse", "Pulse", "pulse"), { type: "nav-row", key: "pulse", label: "Pulse", glyph: "pulse" });
  assert.equal(dragType(navRowDrag("inbox", "Inbox", "inbox")), "nav-row");
  assert.ok(isDragOf(navRowDrag("inbox", "Inbox", "inbox"), "nav-row"));
  assert.ok(!isDragOf(navRowDrag("inbox", "Inbox", "inbox"), "rail-row"), "a destination is not a rail row");
});

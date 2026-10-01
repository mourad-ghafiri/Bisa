/**
 * The explorer's verbs as facts. Run with `npm test` from `desktop/`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  basename,
  deleteCopy,
  fanOutSummary,
  menuSpec,
  nextCopyName,
  renameError,
  renameSelection,
  revealLabel,
  splitExt,
  targetDir,
  viewportMenuSpec,
} from "./fileTreeMutations.mjs";

test("a duplicate is named the way a file manager names it, never a name a sibling holds", () => {
  assert.equal(nextCopyName("foo.txt", ["foo.txt"]), "foo copy.txt");
  assert.equal(nextCopyName("foo.txt", ["foo.txt", "foo copy.txt"]), "foo copy 2.txt");
  assert.equal(nextCopyName("foo.txt", ["foo.txt", "foo copy.txt", "foo copy 2.txt"]), "foo copy 3.txt");
  assert.equal(nextCopyName(".env", [".env"]), ".env copy", "a dot-file keeps its whole name");
  assert.equal(nextCopyName("src", ["src"]), "src copy", "a folder likewise");
  assert.equal(nextCopyName("archive.tar.gz", ["archive.tar.gz"]), "archive.tar copy.gz");
  assert.deepEqual(splitExt("a.b.c"), ["a.b", ".c"]);
  assert.deepEqual(splitExt("Makefile"), ["Makefile", ""]);
});

test("a rename selects the stem so the extension survives a replacement", () => {
  assert.deepEqual(renameSelection("main.rs"), [0, 4]);
  assert.deepEqual(renameSelection(".gitignore"), [0, 10]);
  assert.equal(basename("a/b/c.txt"), "c.txt");
  assert.equal(basename("c.txt"), "c.txt");
  assert.equal(targetDir("src/lib.rs", false), "src");
  assert.equal(targetDir("src", true), "src");
  assert.equal(targetDir("README.md", false), "");
});

test("the delete confirmation promises exactly what the node will do, for one entry or many", () => {
  const file = (path) => ({ path, dir: false });
  const dir = (path) => ({ path, dir: true });
  const trash = deleteCopy({ targets: [file("a.txt")], counts: {}, disposal: "trash", gitRoot: true });
  assert.equal(trash.title, "Delete a.txt?");
  assert.match(trash.body, /moves to the Trash/);
  assert.equal(trash.confirm, "Move to Trash");
  assert.equal(trash.danger, false);
  const unlink = deleteCopy({ targets: [dir("build")], counts: { build: 3 }, disposal: "unlink", gitRoot: true });
  assert.equal(unlink.title, "Delete the folder build?");
  assert.match(unlink.body, /holds 3 entries at the top level/);
  assert.match(unlink.body, /index still holds/);
  assert.equal(unlink.confirm, "Delete");
  assert.equal(unlink.danger, true);
  assert.match(deleteCopy({ targets: [dir("x")], counts: { x: null }, disposal: null, gitRoot: false }).body, /Counting.*Checking/);
  assert.match(deleteCopy({ targets: [dir("x")], counts: { x: -1 }, disposal: "unlink", gitRoot: false }).body, /Everything inside it goes too/);
  assert.match(deleteCopy({ targets: [dir("x")], counts: { x: 0 }, disposal: "unlink", gitRoot: false }).body, /It is empty/);

  const many = deleteCopy({ targets: [file("a.txt"), dir("src"), dir("docs"), file("b"), file("c"), file("d")], counts: { src: 4, docs: 0 }, disposal: "trash", gitRoot: false });
  assert.equal(many.title, "Delete 6 items?");
  assert.match(many.body, /^a\.txt, src, docs, b, c and 1 more\. 2 folders hold 4 entries at the top level/);
  assert.match(deleteCopy({ targets: [file("a"), dir("src")], counts: { src: 0 }, disposal: "trash", gitRoot: false }).body, /The folder src is empty\./);
  assert.match(deleteCopy({ targets: [file("a"), dir("src")], counts: { src: 2 }, disposal: "trash", gitRoot: false }).body, /The folder src holds 2 entries/);
  assert.match(deleteCopy({ targets: [dir("a"), dir("b")], counts: { a: -1, b: 0 }, disposal: "trash", gitRoot: false }).body, /Everything inside the folders goes too/);
  assert.match(deleteCopy({ targets: [dir("a"), dir("b")], counts: { a: 0, b: 0 }, disposal: "trash", gitRoot: false }).body, /The folders are empty/);
});

test("a fan-out's words say what was done and, when it stopped, what was not", () => {
  assert.deepEqual(fanOutSummary({ verb: "deleted", done: ["a", "b"], failed: null, total: 2 }), { ok: true, text: "2 deleted." });
  assert.deepEqual(fanOutSummary({ verb: "moved", done: ["a"], failed: { path: "src/b.rs", reason: "locked" }, total: 4 }), {
    ok: false,
    text: "1 of 4 moved — b.rs: locked; the 2 after it were left alone.",
  });
  assert.equal(fanOutSummary({ verb: "copied", done: [], failed: { path: "x", reason: "no" }, total: 2 }).text, "0 of 2 copied — x: no; the 1 after it was left alone.");
  assert.equal(fanOutSummary({ verb: "copied", done: ["a"], failed: { path: "x", reason: "no" }, total: 2 }).text, "1 of 2 copied — x: no.");
});

test("the row menu offers the verbs in order; Reveal only where a file manager is; a read-only tree only the paths; several rows only the verbs a set can take", () => {
  const base = { desktop: true, mutable: true, clipboard: "tree", reveal: "Reveal in Finder" };
  const file = { path: "a.txt", dir: false };
  const ids = (args) => menuSpec({ ...base, targets: [file], ...args }).map((i) => i.id);
  assert.deepEqual(ids({}), ["new-file", "new-dir", "rename", "duplicate", "cut", "copy", "paste", "copy-path", "copy-absolute", "reveal", "delete"]);
  assert.deepEqual(ids({ desktop: false }), ["new-file", "new-dir", "rename", "duplicate", "cut", "copy", "paste", "copy-path", "copy-absolute", "delete"]);
  assert.deepEqual(ids({ mutable: false }), ["copy-path", "copy-absolute", "reveal"]);
  assert.deepEqual(ids({ serve: true, targets: [{ path: "public", dir: true }] }), ["new-file", "new-dir", "rename", "duplicate", "cut", "copy", "paste", "copy-path", "copy-absolute", "reveal", "serve", "delete"], "a folder of a checkout can be served (ide/18)");
  assert.ok(!ids({ serve: true, targets: [{ path: "a.txt", dir: false }] }).includes("serve"), "a file is not served");
  const items = menuSpec({ ...base, targets: [{ path: "src", dir: true }] });
  assert.equal(items.at(-1).label, "Delete folder…");
  assert.ok(items.at(-1).danger);
  assert.equal(items.at(-1).command, "delete_entry", "the menu names the keymap command, so it can show the chord");
  assert.ok(items.find((i) => i.id === "rename").separatorBefore);
  assert.equal(items.find((i) => i.id === "paste").label, "Paste into folder");
  assert.equal(menuSpec({ ...base, targets: [file] }).find((i) => i.id === "paste").label, "Paste beside");
  assert.ok(menuSpec({ ...base, targets: [file], clipboard: null }).find((i) => i.id === "paste").disabled, "nothing to paste: offered, disabled");
  assert.equal(menuSpec({ ...base, targets: [file], clipboard: "os" }).find((i) => i.id === "paste").label, "Paste from the file manager beside", "a copy made in Finder says where it comes from");
  assert.equal(menuSpec({ ...base, targets: [{ path: "src", dir: true }], clipboard: "os" }).find((i) => i.id === "paste").label, "Paste from the file manager into folder");
  assert.equal(menuSpec({ ...base, targets: [{ path: "src", dir: true }], clipboard: "image" }).find((i) => i.id === "paste").label, "Paste the picture into folder", "a picture on the clipboard says so");
  assert.equal(menuSpec({ ...base, targets: [file], clipboard: "image" }).find((i) => i.id === "paste").label, "Paste the picture beside");
  assert.equal(menuSpec({ ...base, targets: [file], reveal: "Show in File Explorer" }).find((i) => i.id === "reveal").label, "Show in File Explorer");
  assert.ok(!menuSpec({ ...base, targets: [file], mutable: false }).find((i) => i.id === "copy-path").separatorBefore, "the first item never draws a rule");

  const three = menuSpec({ ...base, targets: [file, { path: "src", dir: true }, { path: "z", dir: false }] });
  assert.deepEqual(
    three.map((i) => [i.id, i.label]),
    [
      ["duplicate", "Duplicate 3"],
      ["cut", "Cut 3"],
      ["copy", "Copy 3"],
      ["copy-path", "Copy relative paths"],
      ["copy-absolute", "Copy absolute paths"],
      ["delete", "Delete 3 items…"],
    ],
  );
  assert.ok(three[0].separatorBefore === undefined && three.find((i) => i.id === "copy-path").separatorBefore, "the rules stay where the groups change");
  assert.deepEqual(
    menuSpec({ ...base, targets: [file, { path: "z", dir: false }], mutable: false }).map((i) => i.id),
    ["copy-path", "copy-absolute"],
    "read-only, several: the paths and nothing else",
  );
});

test("the background menu is the root's: new, paste, reveal the root, refresh", () => {
  const ids = (args) => viewportMenuSpec({ desktop: true, mutable: true, clipboard: null, reveal: "Reveal in Finder", ...args }).map((i) => i.id);
  assert.deepEqual(ids({}), ["new-file", "new-dir", "paste", "reveal", "refresh"]);
  assert.deepEqual(ids({ mutable: false, desktop: false }), ["refresh"]);
  assert.deepEqual(ids({ mutable: false }), ["reveal", "refresh"]);
  const items = viewportMenuSpec({ desktop: true, mutable: true, clipboard: null, reveal: "Reveal in Finder" });
  assert.ok(items.find((i) => i.id === "paste").disabled);
  assert.equal(viewportMenuSpec({ desktop: true, mutable: true, clipboard: "os", reveal: "Reveal in Finder" }).find((i) => i.id === "paste").label, "Paste from the file manager", "the root takes a Finder copy too");
  assert.equal(viewportMenuSpec({ desktop: true, mutable: true, clipboard: "tree", reveal: "Reveal in Finder" }).find((i) => i.id === "paste").label, "Paste");
  assert.equal(viewportMenuSpec({ desktop: true, mutable: true, clipboard: "image", reveal: "Reveal in Finder" }).find((i) => i.id === "paste").label, "Paste the picture");
  assert.equal(items.find((i) => i.id === "reveal").label, "Reveal in Finder (root)");
});

test("the reveal label names the platform's file manager, never Finder by default", () => {
  assert.equal(revealLabel("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"), "Reveal in Finder");
  assert.equal(revealLabel("Mozilla/5.0 (Windows NT 10.0; Win64; x64)"), "Reveal in File Explorer");
  assert.equal(revealLabel("Mozilla/5.0 (X11; Linux x86_64)"), "Reveal in file manager");
  assert.equal(revealLabel(""), "Reveal in file manager");
  assert.equal(revealLabel(null), "Reveal in file manager", "one verb on every platform; the place is the platform's");
});

test("an inline rename refuses nothing, a slash, dots and a sibling's name — and accepts its own", () => {
  const sib = ["a.txt", "b.txt"];
  assert.equal(renameError("", "a.txt", sib), "A name is needed.");
  assert.equal(renameError("  ", "a.txt", sib), "A name is needed.");
  assert.match(renameError("x/y", "a.txt", sib), /slash/);
  assert.equal(renameError("..", "a.txt", sib), "That is not a name.");
  assert.equal(renameError("b.txt", "a.txt", sib), "b.txt is already here.");
  assert.equal(renameError("a.txt", "a.txt", sib), null, "its own name is not a collision");
  assert.equal(renameError("c.txt", "a.txt", sib), null);
});

test("every verb the menu can offer is run by the mutations hook — a verb with no hand is a dead menu row", async () => {
  const { readFileSync } = await import("node:fs");
  const hook = readFileSync(new URL("./useTreeMutations.tsx", import.meta.url), "utf8");
  const run = hook.slice(hook.indexOf("const run = (menuId: MenuId"), hook.indexOf("\n  };", hook.indexOf("const run = (menuId: MenuId")));
  const ids = new Set();
  const every = { desktop: true, mutable: true, clipboard: "image", reveal: "Reveal in Finder", serve: true };
  for (const targets of [[{ path: "a.txt", dir: false }], [{ path: "src", dir: true }], [{ path: "a", dir: false }, { path: "b", dir: false }]]) {
    for (const item of menuSpec({ ...every, targets })) ids.add(item.id);
  }
  for (const id of ids) assert.ok(run.includes(`case "${id}":`), `the hook runs ${id}`);
  assert.ok(ids.size >= 12, `the menu offers ${ids.size} verbs`);
});

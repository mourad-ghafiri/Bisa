/**
 * Pasting the machine's clipboard into the explorer. Run with
 * `node --test desktop/src/ui/osPasteModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { pasteDestination, pasteRefusal, pasteSource, pasteWords } from "./osPasteModel.mjs";

test("a paste lands in the root or the folder under the cursor, by absolute path, and nowhere without a root", () => {
  assert.equal(pasteDestination("/w/repo", ""), "/w/repo");
  assert.equal(pasteDestination("/w/repo/", ""), "/w/repo/", "the root as given");
  assert.equal(pasteDestination("/w/repo", "src/lib"), "/w/repo/src/lib");
  assert.equal(pasteDestination("/w/repo/", "/src/"), "/w/repo/src", "no doubled slashes");
  assert.equal(pasteDestination(null, "src"), null, "no root known yet: nowhere to copy");
});

test("the toast counts what landed, what was renamed and what was left out, and says nothing landed when nothing did", () => {
  const p = (from, name) => ({ from, name });
  assert.deepEqual(pasteWords({ pasted: [p("/x/a.txt", "a.txt")], skipped: [] }, "src"), { text: "Pasted a.txt into src/.", tone: "ok" });
  assert.deepEqual(pasteWords({ pasted: [p("/x/a.txt", "a.txt"), p("/x/site", "site 2")], skipped: [] }, ""), { text: "Pasted 2 items into the root (1 renamed).", tone: "ok" });
  assert.deepEqual(
    pasteWords({ pasted: [p("/x/a.txt", "a.txt")], skipped: [{ path: "/x/link", why: "a link, not copied" }] }, "src/"),
    { text: "Pasted a.txt into src/ — 1 left out: a link, not copied.", tone: "warn" },
  );
  assert.deepEqual(
    pasteWords({ pasted: [p("/x/a", "a")], skipped: [{ path: "/x/l", why: "a link, not copied" }, { path: "/x/g", why: "not there" }] }, ""),
    { text: "Pasted a into the root — 2 left out.", tone: "warn" },
  );
  assert.deepEqual(pasteWords({ pasted: [], skipped: [{ path: "/x/l", why: "a link, not copied" }] }, "src"), { text: "Nothing pasted — a link, not copied.", tone: "error" });
  assert.deepEqual(pasteWords({ pasted: [], skipped: [] }, "src"), { text: "Nothing pasted.", tone: "error" });
});

test("the paste's source is the tree's clipboard first, else the file manager's files, else the picture, else nothing — and the refusal says what to do", () => {
  const held = (files, image) => ({ files, image });
  assert.equal(pasteSource({ kind: "copy" }, held(true, true)), "tree");
  assert.equal(pasteSource(null, held(true, false)), "os");
  assert.equal(pasteSource(null, held(true, true)), "os", "a copied image file is a file, under its own name");
  assert.equal(pasteSource(null, held(false, true)), "image");
  assert.equal(pasteSource(null, held(false, false)), null);
  assert.match(pasteRefusal(), /file manager/);
  assert.match(pasteRefusal(), /picture/);
});

test("a pasted picture's toast reads as one item landed, never renamed", () => {
  assert.deepEqual(pasteWords({ pasted: [{ from: "the clipboard", name: "login-bug.png" }], skipped: [] }, "docs"), { text: "Pasted login-bug.png into docs/.", tone: "ok" });
  assert.deepEqual(pasteWords({ pasted: [{ from: "/Users/a/Desktop/login-bug.png", name: "login-bug 2.png" }], skipped: [] }, "docs"), { text: "Pasted login-bug 2.png into docs/ (1 renamed).", tone: "ok" }, "a file that took a new name to land is a rename");
});

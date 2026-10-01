/**
 * A file as a person works it in the Project IDE (ide/03, ide/11, ide/12,
 * ide/17): it is found by name as the index follows the disk, opened, typed
 * into and saved by compare-and-swap; two writers on it meet as a conflict
 * that loses nothing; a file over the bounds this machine set is refused or
 * read-only in the node's words and numbers; reads that answer out of order
 * leave the newest on screen; a path in any text is a door with the verbs
 * its card holds; a relative link in a rendered document stays inside the
 * root; a diagram's error points at the line in the person's file; nine
 * projects later, what was typed and not saved is still there. Stepped
 * through the models the way the components do; no DOM.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs src/scenarios/files.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { createLatest } from "../shell/latestModel.mjs";
import { pathCard, urlCard } from "../shell/linkCardModel.mjs";
import { patchIndex, rankPaths } from "../shell/quickOpenScore.mjs";
import { linksAt } from "../terminal/terminalLinksModel.mjs";
import { bytesFailure } from "../ui/artifact/fileBytesModel.mjs";
import { matchesOf, replaceAll } from "../ui/find/findModel.mjs";
import { findLinks, resolveLink } from "../ui/linkModel.mjs";
import { mermaidBlocks, offsetError } from "../ui/mermaidModel.mjs";
import { guardWords } from "../views/_workbench/closeGuardModel.mjs";
import { resolveDocLink } from "../views/_workbench/docLink.mjs";
import { changedOnDisk, conflicted, edited, emptyBuffer, isUnsaved, keepMine, loadFailed, loadFailure, loaded, refusalWords, saveFailure, saved, sizeNote, unsavedKeys } from "../views/_workbench/editorModel.mjs";
import { defaultMode, docKindOf, docModes, isRenderedDoc, tooLargeWords } from "../views/_workbench/fileDocModel.mjs";
import { nameResults, parseQuery } from "../views/_workbench/fileSearchModel.mjs";
import { MAX_ROOTS, closedTabs, emptyWorkbench, heldRoots, openTab, rootKey, tabId, tabsFor } from "../views/_workbench/workbenchModel.mjs";
import { misfits } from "./schemaFit.mjs";

const SCHEMA = JSON.parse(readFileSync(new URL("../../api-schema.json", import.meta.url), "utf8"));
const MIB = 1024 * 1024;
const refused = (status, message, body) => Object.assign(new Error(message), { status, body });
const onDisk = (text, extra = {}) => ({ text, hash: `sha(${text})`, editable: true, truncated: false, binary: false, size: text.length, ...extra });

test("found by name: the index follows the disk frame by frame, and quick open ranks what it holds", () => {
  let index = ["README.md", "src/main.rs", "src/lib.rs"];
  // An agent writes a file, a build fills `target/`, a commit moves git's own files, a folder is copied in.
  index = patchIndex(index, { kind: "created", path: "src/parser.rs", ignored: false, dir: false });
  const afterBuild = patchIndex(index, { kind: "created", path: "target/debug/app", ignored: true, dir: false });
  assert.equal(afterBuild, index, "what the ignore rules match adds no row");
  assert.equal(patchIndex(index, { kind: "created", path: ".git/index", ignored: false }), index);
  assert.equal(patchIndex(index, { kind: "created", path: "vendor", ignored: false, dir: true }), null, "a folder made whole is read again: its files were named by no frame");
  index = patchIndex(index, { kind: "renamed", from: "src", path: "core", dir: true });
  assert.deepEqual(index, ["README.md", "core/lib.rs", "core/main.rs", "core/parser.rs"]);
  assert.equal(rankPaths("parser", index)[0].path, "core/parser.rs");
  assert.deepEqual(nameResults(index, "lib"), ["core/lib.rs"]);
  assert.deepEqual(parseQuery("total in:src/** -in:*.lock"), { needle: "total", include: ["src/**"], exclude: ["*.lock"] });
});

test("how a file opens is decided from its path: the editor, the editor with a rendering, or a rendering alone", () => {
  assert.deepEqual([docKindOf("src/main.rs"), docModes("text"), defaultMode("text")], ["text", [], "source"]);
  assert.deepEqual([docKindOf("README.md"), defaultMode("markdown")], ["markdown", "rendered"]);
  assert.deepEqual([docKindOf("flow.mmd"), docModes("diagram")], ["diagram", ["rendered", "split", "source"]]);
  assert.equal(defaultMode(docKindOf("index.html")), "source", "a repository's page rarely carries its assets");
  assert.equal(isRenderedDoc(docKindOf("report.pdf")), true, "bytes drawn as they are: no editor behind them");
  assert.equal(isRenderedDoc(docKindOf("data.csv")), false, "a sheet by text stays the editor's");
});

test("typed, saved by compare-and-swap, and met by another writer: a conflict that loses nothing", () => {
  let b = loaded(emptyBuffer("src/main.rs"), onDisk("fn main() {}"));
  b = edited(b, "fn main() { run(); }", 1000);
  assert.equal(isUnsaved(b), true);
  // The body of the save names what was read; the node takes nothing else.
  assert.deepEqual(misfits(SCHEMA, "IdeWriteFile", { text: b.text, base_hash: b.savedHash }), []);
  assert.notDeepEqual(misfits(SCHEMA, "IdeWriteFile", { text: b.text, base_hash: b.savedHash, path: "src/main.rs" }), [], "a key the node does not declare is refused");
  b = saved(b, b.text, "sha(2)");
  assert.deepEqual([b.status, b.savedHash], ["clean", "sha(2)"]);
  // An agent writes the file while the person types again: the save meets a 409 that carries the current text.
  b = edited(b, "fn main() { run(); stop(); }", 2000);
  const met = saveFailure(refused(409, "src/main.rs changed since you read it", { current_hash: "sha(3)", current_text: "fn main() { start(); }" }));
  assert.deepEqual(met, { kind: "conflict", text: "fn main() { start(); }", hash: "sha(3)" });
  b = conflicted(b, met.text, met.hash);
  assert.equal(b.text, "fn main() { run(); stop(); }", "every character typed is still there");
  b = keepMine(b);
  assert.deepEqual([b.status, b.savedHash], ["dirty", "sha(3)"], "the next save is against what is on disk now");
  // The editor's own save, announced by the watcher, is no change at all.
  const own = saved(b, b.text, "sha(4)");
  assert.equal(changedOnDisk(own, onDisk(own.text, { hash: "sha(4)" })), own);
});

test("the editor's bounds are this machine's: read-only and refused are the node's words, with the node's numbers", () => {
  // `editor.large_file.editable_mib` lowered to 1: the node calls a 1.5 MiB file not editable.
  const lowered = loaded(emptyBuffer("big.log"), onDisk("…", { size: 1.5 * MIB, editable: false }));
  assert.equal(lowered.readOnly, true);
  assert.equal(edited(lowered, "typed", 1), lowered, "a read-only buffer takes no keystroke");
  assert.match(sizeNote(lowered), /above the editable size this machine set/);
  // Raised to 8: a 5 MiB file is typed into, drawn plain.
  const raised = loaded(emptyBuffer("big.log"), onDisk("…", { size: 5 * MIB, editable: true }));
  assert.deepEqual([raised.readOnly, raised.plain], [false, true]);
  // `editor.large_file.refuse_mib` set to 8: a 12 MiB file is refused with the limit in force.
  const refusal = loadFailure(refused(413, "big.log is 12582912 bytes; the editor stops at 8388608", { size: 12 * MIB, limit: 8 * MIB }));
  const shown = loadFailed(emptyBuffer("big.log"), refusal);
  assert.equal(shown.status, "error");
  assert.match(refusalWords(shown.refusal), /^12\.0 MB — the editor on this machine stops at 8\.0 MB/);
  // A save of a text that grew past the editable size says the same bound.
  assert.match(saveFailure(refused(413, "…", { size: 3 * MIB, limit: MIB })).message, /edits up to 1\.0 MB/);
  // A rendering of bytes: the limit the node said, never one kept here.
  const bytes = bytesFailure(refused(413, "…", { size: 300 * MIB, limit: 256 * MIB }));
  assert.match(tooLargeWords(bytes.size, bytes.limit), /^300\.0 MB — rendering stops at 256\.0 MB/);
});

test("reads that answer out of order leave the newest on screen, and a tab that closed is written by none", () => {
  const reads = createLatest();
  let b = emptyBuffer("notes.md");
  const land = (ticket, file) => {
    if (reads.lands(ticket)) b = b.status === "loading" ? loaded(b, file) : changedOnDisk(b, file);
  };
  const onMount = reads.begin();
  const afterWrite = reads.begin();
  land(afterWrite, onDisk("v2"));
  land(onMount, onDisk("v1"));
  assert.equal(b.text, "v2", "the read of the file as it was answered last, and did not land");
  // A re-read that fails under typed work leaves the screen to the editor.
  b = edited(b, "v2 and mine", 1);
  const failed = reads.begin();
  if (reads.lands(failed)) b = loadFailed(b, loadFailure(new Error("the node did not answer")));
  assert.deepEqual([b.status, b.text], ["dirty", "v2 and mine"]);
  // The tab closes while a read is out.
  const late = reads.begin();
  reads.close();
  assert.equal(reads.lands(late), false);
});

test("a path in any text is a door: found, resolved against the roots, and offered as a card's verbs", () => {
  const roots = [{ scope: "workstream", id: "w1", root: "/Users/me/app", label: "App", paths: ["src/main.rs", "README.md"] }];
  const text = "error[E0425] at src/main.rs:42:7 — see https://example.com/docs and «secret:token:ab12»/x.rs";
  const found = findLinks(text);
  assert.deepEqual(found.map((l) => l.kind), ["path", "url"], "a redacted secret is never a link, whatever follows it");
  const [path, url] = found;
  const card = pathCard(path, resolveLink(path, roots), { reveal: "Reveal in Finder" });
  assert.deepEqual(card.verbs.map((v) => v.id), ["open", "reveal", "copy"]);
  assert.equal(card.verbs[0].label, "Open src/main.rs:42:7 in the IDE");
  assert.deepEqual(urlCard(url.url, { embedded: false }).verbs.map((v) => v.id), ["open_machine", "copy"], "a URL asks before any browser opens");
  // The same words in a terminal, cut by the column edge, are one link across its rows.
  const rows = ["see /Users/me/app/src/ma", "in.rs:42 for the rest"];
  const rowAt = (y) => (rows[y] === undefined ? null : { text: rows[y], wrapped: y === 1 });
  const onFirst = linksAt(rowAt, 0, 24).find((l) => l.hit.kind === "path");
  assert.deepEqual([onFirst.hit.path, onFirst.hit.line], ["/Users/me/app/src/main.rs", 42]);
  assert.deepEqual([onFirst.range.start.y, onFirst.range.end.y], [1, 2], "underlined across both rows");
  assert.deepEqual(linksAt(rowAt, 1, 24).find((l) => l.hit.kind === "path").hit, onFirst.hit, "a ⌘-click on either row opens the same door");
});

test("a relative link in a rendered document stays inside the root, and a diagram's error names the line in the person's file", () => {
  assert.equal(resolveDocLink("docs/guide/intro.md", "./setup.md"), "docs/guide/setup.md");
  assert.equal(resolveDocLink("docs/guide/intro.md", "../api/index.md#L12"), "docs/api/index.md");
  assert.equal(resolveDocLink("README.md", "../outside.md"), null, "one that climbs out of the root is inert");
  assert.equal(resolveDocLink("README.md", "https://example.com"), null, "a URL is the link handler's");
  const markdown = ["# Flow", "", "text", "", "```mermaid", "graph TD", "  A --> B", "```", ""].join("\n");
  const [block] = mermaidBlocks(markdown);
  assert.equal(block.startLine, 6, "the first line of the diagram, counted in the document");
  assert.deepEqual(offsetError("Parse error on line 2: expected an arrow", block.startLine), { line: 7, message: "Parse error on line 7: expected an arrow" });
});

test("find in a rendering replaces into the buffer, so the tab dirties like any edit", () => {
  let b = loaded(emptyBuffer("README.md"), onDisk("cart_total and cart_total again"));
  const find = { query: "cart_total", regex: false, caseSensitive: false, replacement: "basket_total" };
  assert.equal(matchesOf(b.text, find).length, 2);
  b = edited(b, replaceAll(b.text, find).text, 5);
  assert.deepEqual([b.text, b.status], ["basket_total and basket_total again", "dirty"]);
  assert.equal(matchesOf(b.text, { ...find, query: "(", regex: true }).length, 0, "an expression that does not compile finds nothing and throws nothing");
});

test("nine projects later, what was typed and not saved is still there — and closing it asks by name", () => {
  const root = (i) => rootKey("workstream", `w${i}`);
  const file = (path) => ({ kind: "file", path });
  const buffers = new Map([[`${root(0)}|file:notes.md`, edited(loaded(emptyBuffer("notes.md"), onDisk("")), "an idea", 1)]]);
  let state = openTab(emptyWorkbench(), root(0), file("notes.md"), { held: heldRoots(unsavedKeys(buffers)) });
  for (let i = 1; i <= MAX_ROOTS + 2; i += 1) {
    const before = state;
    state = openTab(state, root(i), file("main.rs"), { held: heldRoots(unsavedKeys(buffers)) });
    assert.ok(!closedTabs(before, state).some(([key, id]) => key === root(0) && id === "file:notes.md"), `opening the ${i}th root closed the unsaved document`);
  }
  assert.deepEqual(tabsFor(state, root(0)).map(tabId), ["file:notes.md"]);
  assert.ok(!state.roots.some((r) => r.key === root(1)), "a root with nothing unsaved went, coldest first");
  assert.equal(guardWords([{ label: "notes.md", untitled: false }]).title, "Save changes to notes.md?");
});

/**
 * How a file opens in the centre. Run with `node --test desktop/src/views/_workbench/fileDocModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { tabId } from "./workbenchModel.mjs";
import { artifactKindOf, binaryWords, defaultMode, docKindOf, docModeKey, docModes, isRenderedDoc, modeGlyph, modeLabel, renderedLabel, tooLargeWords } from "./fileDocModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("the byte cap is the node's alone: the model keeps no bound of its own, and the words say the limit the node said", () => {
  const rust = readFileSync(join(here, "../../../../crates/bisa-engine/src/ide/files.rs"), "utf8");
  assert.match(rust, /pub const RAW_REFUSE_BYTES: u64 = \d+ \* 1024 \* 1024;/, "the engine names the cap, and answers it in the refusal");
  const source = readFileSync(join(here, "fileDocModel.mjs"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.ok(!/\d+ \* 1024/.test(source), "no size is spelled here");
  assert.ok(!/MiB|\bMB\b/.test(source), "nor a bound in its words");
});

test("a path opens as the editor, the editor with a rendered view, or a rendered document — by its extension alone", () => {
  assert.equal(docKindOf("README.md"), "markdown");
  assert.equal(docKindOf("docs/flow.mmd"), "diagram");
  assert.equal(docKindOf("data/q3.csv"), "sheet_text", "a csv is text the editor holds, rendered as a grid");
  assert.equal(docKindOf("data/q3.TSV"), "sheet_text", "case does not matter");
  assert.equal(docKindOf("book.xlsx"), "sheet");
  assert.equal(docKindOf("old.xls"), "sheet");
  assert.equal(docKindOf("mark.svg"), "svg");
  assert.equal(docKindOf("index.html"), "html");
  assert.equal(docKindOf("report.pdf"), "pdf");
  assert.equal(docKindOf("memo.docx"), "document");
  assert.equal(docKindOf("deck.pptx"), "slides");
  assert.equal(docKindOf("shot.png"), "image");
  assert.equal(docKindOf("demo.mp4"), "video");
  assert.equal(docKindOf("talk.mp3"), "audio");
  assert.equal(docKindOf("src/main.rs"), "text");
  assert.equal(docKindOf("Makefile"), "text");
  assert.equal(docKindOf("archive.zip"), "text", "a kind nothing renders is the editor's to refuse as binary");
  assert.equal(docKindOf(""), "text");
});

test("the modes per kind, the one each opens on, and which are documents with no editor", () => {
  assert.deepEqual(docModes("markdown"), ["rendered", "split", "source"]);
  assert.deepEqual(docModes("diagram"), ["rendered", "split", "source"]);
  assert.deepEqual(docModes("sheet_text"), ["rendered", "source"]);
  assert.deepEqual(docModes("svg"), ["rendered", "source"]);
  assert.deepEqual(docModes("html"), ["rendered", "split", "source"], "a page: its source and its rendering side by side, too");
  for (const k of ["pdf", "sheet", "document", "slides", "image", "video", "audio"]) {
    assert.deepEqual(docModes(k), ["rendered"], k);
    assert.ok(isRenderedDoc(k), `${k} is a rendered document`);
  }
  assert.deepEqual(docModes("text"), []);
  assert.ok(!isRenderedDoc("markdown") && !isRenderedDoc("text") && !isRenderedDoc("svg"));
  assert.equal(defaultMode("markdown"), "rendered");
  assert.equal(defaultMode("sheet_text"), "rendered");
  assert.equal(defaultMode("html"), "source", "a repository's page rarely carries its assets: it opens on its source");
  assert.equal(defaultMode("pdf"), "rendered");
  assert.equal(defaultMode("text"), "source");
  assert.equal(renderedLabel("diagram"), "Preview");
  assert.equal(renderedLabel("markdown"), "Rendered");
  // The control is glyphs alone: the word is the tooltip, the glyph a name in the one glyph map.
  assert.equal(modeLabel("diagram", "rendered"), "Preview");
  assert.equal(modeLabel("html", "rendered"), "Rendered");
  assert.equal(modeLabel("html", "split"), "Split");
  assert.equal(modeLabel("markdown", "source"), "Source");
  assert.deepEqual(["rendered", "split", "source"].map(modeGlyph), ["rendered", "splitRight", "code"]);
  assert.equal(docModeKey("workstream:w1", "docs/README.md"), `workstream:w1|${tabId({ kind: "file", path: "docs/README.md" })}`, "remembered under the document's own key: the root and the file's tab");
  assert.equal(artifactKindOf("sheet_text"), "sheet");
  assert.equal(artifactKindOf("pdf"), "pdf");
  assert.equal(artifactKindOf("text"), "file");
});

test("the refusals say the size, the bound and the way out", () => {
  assert.match(tooLargeWords(300 * 1024 * 1024, 256 * 1024 * 1024), /^300\.0 MB — rendering stops at 256\.0 MB\. Reveal it/);
  assert.match(tooLargeWords(90 * 1024 * 1024, 64 * 1024 * 1024), /rendering stops at 64\.0 MB/, "whatever limit the node said");
  assert.equal(tooLargeWords(null, null), "Too large to render here. Reveal it and open it with another application.", "an answer that carried no sizes invents none");
  assert.equal(tooLargeWords(300, undefined), "Too large to render here. Reveal it and open it with another application.");
  assert.equal(modeLabel("diagram", "rendered"), "Preview");
  assert.match(binaryWords(9), /^Binary — 9 B of bytes, and no text to show\.$/);
});

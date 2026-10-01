/**
 * The glyph a file name earns. Run with `node --test desktop/src/ui/fileIcons.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { fileIconKind } from "./fileIcons.mjs";

test("a name reads as code, data, text, an image, a PDF, a recording, a sheet, a document, a deck, an archive, configuration or a plain file", () => {
  assert.equal(fileIconKind("main.rs"), "code");
  assert.equal(fileIconKind("App.tsx"), "code");
  assert.equal(fileIconKind("package.json"), "data");
  assert.equal(fileIconKind("Cargo.toml"), "data");
  assert.equal(fileIconKind("README.md"), "text");
  assert.equal(fileIconKind("LICENSE"), "text");
  assert.equal(fileIconKind("logo.PNG"), "image", "case does not matter");
  // The kinds the IDE renders as documents wear their own glyph in the tree and on the tab (ide/03).
  assert.equal(fileIconKind("report.pdf"), "pdf");
  assert.equal(fileIconKind("demo.mp4"), "video");
  assert.equal(fileIconKind("talk.mp3"), "audio");
  assert.equal(fileIconKind("data.csv"), "sheet", "a csv is a sheet, drawn as a grid");
  assert.equal(fileIconKind("book.xlsx"), "sheet");
  assert.equal(fileIconKind("memo.docx"), "document");
  assert.equal(fileIconKind("deck.pptx"), "slides");
  assert.equal(fileIconKind("dist.tar.gz"), "archive");
  assert.equal(fileIconKind(".gitignore"), "config");
  assert.equal(fileIconKind(".env"), "config");
  assert.equal(fileIconKind("Cargo.lock"), "config");
  assert.equal(fileIconKind("Dockerfile"), "config");
  assert.equal(fileIconKind("justfile"), "config");
  assert.equal(fileIconKind("notes"), "file", "no extension, not a known name");
  assert.equal(fileIconKind("thing.unknownext"), "file");
  assert.equal(fileIconKind(""), "file");
  assert.equal(fileIconKind(null), "file");
});

/**
 * `docLink.mjs` — where a link inside a rendered markdown file goes.
 *
 * Run with `npm test` from `desktop/`.
 */

import test from "node:test";
import assert from "node:assert/strict";

import { isMarkdown, resolveDocLink } from "./docLink.mjs";

test("a relative link resolves against the directory of the file it is in", () => {
  assert.equal(resolveDocLink("docs/API.md", "./STUDIO.md"), "docs/STUDIO.md");
  assert.equal(resolveDocLink("docs/API.md", "STUDIO.md"), "docs/STUDIO.md");
  assert.equal(resolveDocLink("docs/API.md", "../README.md"), "README.md");
  assert.equal(resolveDocLink("README.md", "docs/API.md"), "docs/API.md");
  assert.equal(resolveDocLink("a/b/c.md", "../../d.md"), "d.md");
  assert.equal(resolveDocLink("a/b/c.md", "./x/./y.md"), "a/b/x/y.md");
});

test("a link that leaves the scope's root is refused rather than sent", () => {
  // The node would refuse this too, naming the boundary. Refusing here means
  // the link is simply not offered, instead of showing somebody an error.
  assert.equal(resolveDocLink("README.md", "../secrets.md"), null);
  assert.equal(resolveDocLink("docs/API.md", "../../../etc/passwd"), null);
  assert.equal(resolveDocLink("README.md", "/etc/passwd"), null);
});

test("anything that is not a document in this tree is left to the browser", () => {
  for (const href of [
    "https://example.com/x",
    "http://example.com",
    "mailto:someone@example.com",
    "//example.com/x",
    "#a-heading",
    "",
    "   ",
  ]) {
    assert.equal(resolveDocLink("docs/API.md", href), null, href);
  }
  assert.equal(resolveDocLink("docs/API.md", undefined), null);
});

test("a query or a fragment addresses the same file", () => {
  assert.equal(resolveDocLink("docs/API.md", "./STUDIO.md#projects"), "docs/STUDIO.md");
  assert.equal(resolveDocLink("docs/API.md", "./STUDIO.md?v=2"), "docs/STUDIO.md");
});

test("markdown is decided by extension, and only by extension", () => {
  for (const yes of ["README.md", "a/b.MARKDOWN", "notes.mdx", "x/y/z.Md"]) {
    assert.ok(isMarkdown(yes), yes);
  }
  for (const no of ["main.rs", "Makefile", "a.md.bak", "", "notes"]) {
    assert.equal(isMarkdown(no), false, no);
  }
});

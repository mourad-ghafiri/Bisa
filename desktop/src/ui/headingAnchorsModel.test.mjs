/**
 * The anchors a document's headings carry (ide/03 §Rendered documents).
 * Run with `node --test desktop/src/ui/headingAnchorsModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { headingSlug, withHeadingIds } from "./headingAnchorsModel.mjs";

const slug = (text, taken = new Map()) => headingSlug(text, taken);

test("GitHub's rule: lower-case, spaces to hyphens, the rest of the punctuation gone, letters of every script kept", () => {
  assert.equal(slug("Sample Section"), "sample-section");
  assert.equal(slug("This'll be a Helpful Section About the Greek Letter Θ!"), "thisll-be-a-helpful-section-about-the-greek-letter-θ", "GitHub's own example");
  assert.equal(slug("10. Build and upload"), "10-build-and-upload", "a numbered heading keeps its number and loses its dot");
  assert.equal(slug("  Padded  "), "padded", "leading and trailing whitespace trimmed");
  assert.equal(slug("C++ & Rust: a comparison"), "c--rust-a-comparison", "symbols go, the hyphens their spaces became stay");
  assert.equal(slug("snake_case_words"), "snake_case_words", "an underscore stays, as on GitHub");
  assert.equal(slug("Tabs\tand\nnewlines"), "tabsandnewlines", "other whitespace is removed, not hyphened");
  assert.equal(slug("Émilie à Zürich"), "émilie-à-zürich");
  assert.equal(slug("!!!"), "", "no words, no anchor");
});

test("a > inside a heading's quoted attribute does not end the tag, and an inline tag with one is still stripped from the words", () => {
  assert.equal(withHeadingIds('<h2 title="a > b">Hello</h2>'), '<h2 title="a > b" id="hello">Hello</h2>');
  assert.equal(withHeadingIds('<h3><abbr title="1 > 0">One</abbr> two</h3>'), '<h3 id="one-two"><abbr title="1 > 0">One</abbr> two</h3>');
});

test("a heading that repeats an earlier anchor counts up: -1, -2, …", () => {
  const taken = new Map();
  assert.deepEqual([slug("Notes", taken), slug("notes", taken), slug("NOTES!", taken), slug("Other", taken)], ["notes", "notes-1", "notes-2", "other"]);
});

test("every heading of the HTML gets its id — markup stripped, entities read back — and one that has an id or no words is left alone", () => {
  const html = ["<h1>Title</h1>", "<p>intro</p>", "<h2>10. Build <em>and</em> upload</h2>", "<h3><code>cargo</code> &amp; <a href=\"x\">friends</a></h3>", "<h2>10. Build and upload</h2>", '<h4 id="kept">Kept</h4>', "<h5>???</h5>", "<h6 class=\"x\">Six</h6>"].join("");
  assert.equal(
    withHeadingIds(html),
    [
      '<h1 id="title">Title</h1>',
      "<p>intro</p>",
      '<h2 id="10-build-and-upload">10. Build <em>and</em> upload</h2>',
      '<h3 id="cargo--friends"><code>cargo</code> &amp; <a href="x">friends</a></h3>',
      '<h2 id="10-build-and-upload-1">10. Build and upload</h2>',
      '<h4 id="kept">Kept</h4>',
      "<h5>???</h5>",
      '<h6 class="x" id="six">Six</h6>',
    ].join(""),
  );
  assert.equal(withHeadingIds("<p>no heading</p><div>h2</div>"), "<p>no heading</p><div>h2</div>", "nothing but a heading is touched");
  assert.equal(withHeadingIds(""), "");
  assert.equal(withHeadingIds("<h2>&#x1F600; &#65; b</h2>"), '<h2 id="😀-a-b">&#x1F600; &#65; b</h2>', "a numeric entity is read back; an emoji is not punctuation and stays, as on GitHub");
});

/**
 * What a rendered document may carry: the sanitizer's profile, the one input
 * it keeps, a file's front matter left out, and how a tag is read.
 * Run with `node --test desktop/src/ui/markdownHtmlModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { PROSE_PROFILE, TAG_REST, isGfmTaskBox, withoutFrontMatter } from "./markdownHtmlModel.mjs";

test("the profile is DOMPurify's HTML allow-list with nothing that runs, styles, frames, submits, plays, pops over or hides — and no data attribute a README could mint", () => {
  assert.deepEqual(PROSE_PROFILE.USE_PROFILES, { html: true });
  assert.equal(PROSE_PROFILE.ALLOW_DATA_ATTR, false, "data-link, data-path, data-doc-src, data-scroll-keep, data-document are the app's own");
  for (const tag of ["script", "style", "iframe", "object", "embed", "svg", "math", "form", "button", "input".replace("input", "textarea"), "select", "video", "audio", "canvas", "marquee", "dialog", "template", "link", "meta", "base"]) {
    assert.ok(PROSE_PROFILE.FORBID_TAGS.includes(tag), `${tag} never renders`);
  }
  for (const attr of ["style", "srcset", "poster", "background", "autoplay", "hidden", "tabindex", "download", "popover", "popovertarget", "command", "commandfor"]) {
    assert.ok(PROSE_PROFILE.FORBID_ATTR.includes(attr), `${attr} is never kept`);
  }
  assert.ok(!PROSE_PROFILE.FORBID_TAGS.includes("input"), "GFM's task-list box is an input — the hook keeps that one alone");
  for (const kept of ["details", "summary", "img", "table", "kbd", "sub", "sup", "br", "p", "a", "code", "pre", "blockquote", "h1", "ul", "ol", "li"]) {
    assert.ok(!PROSE_PROFILE.FORBID_TAGS.includes(kept), `${kept} renders, as on GitHub`);
  }
  assert.ok(!("ADD_TAGS" in PROSE_PROFILE) && !("ADD_ATTR" in PROSE_PROFILE), "nothing is added to the library's list — a comment stays what it is, never a tag");
  assert.ok(Object.isFrozen(PROSE_PROFILE) && Object.isFrozen(PROSE_PROFILE.FORBID_TAGS), "one profile, never edited in place");
});

test("the one input a document keeps is GFM's task-list box: a disabled checkbox, and nothing else", () => {
  assert.equal(isGfmTaskBox("checkbox", ""), true, "disabled is present, empty");
  assert.equal(isGfmTaskBox("CHECKBOX", "disabled"), true);
  assert.equal(isGfmTaskBox("checkbox", null), false, "a live checkbox is a control");
  assert.equal(isGfmTaskBox("checkbox", undefined), false);
  assert.equal(isGfmTaskBox("text", ""), false);
  assert.equal(isGfmTaskBox(null, ""), false, "no type is a text field");
  assert.equal(isGfmTaskBox("submit", ""), false);
});

test("front matter opens on the first line and closes on --- or ..., CRLF or not, and is left out of the rendering", () => {
  assert.equal(withoutFrontMatter("---\ntitle: Hello\ntags: [a, b]\n---\n# Hello\n\nBody."), "# Hello\n\nBody.");
  assert.equal(withoutFrontMatter("---\r\ntitle: x\r\n---\r\n# Hi\r\n"), "# Hi\r\n", "Windows line endings");
  assert.equal(withoutFrontMatter("---\ntitle: x\n...\nBody"), "Body", "YAML's own document end closes it too");
  assert.equal(withoutFrontMatter("---\ntitle: x\n---"), "", "a file that is only front matter renders nothing");
  assert.equal(withoutFrontMatter("---\ntitle: x\n---\n"), "", "and so with a trailing newline");
});

test("a --- rule later in a file, an unclosed block, and a file with no front matter are content and stay", () => {
  const rule = "# Title\n\nabove\n\n---\n\nbelow";
  assert.equal(withoutFrontMatter(rule), rule, "a thematic break is not front matter");
  const unclosed = "---\ntitle: x\nnever closed";
  assert.equal(withoutFrontMatter(unclosed), unclosed);
  assert.equal(withoutFrontMatter("---"), "---", "one line is a rule");
  assert.equal(withoutFrontMatter(" ---\nx\n---\n"), " ---\nx\n---\n", "indented is not the opener");
  assert.equal(withoutFrontMatter("----\nx\n---\n"), "----\nx\n---\n", "four dashes are not the opener");
  assert.equal(withoutFrontMatter(""), "");
  assert.equal(withoutFrontMatter(null), "");
});

test("a tag is read to its first > outside a double-quoted value, so a > inside an attribute does not end it", () => {
  const tag = new RegExp(`<\\/?([a-zA-Z][a-zA-Z0-9]*)\\b${TAG_REST}>`, "g");
  const html = '<abbr title="a > b">x</abbr> and <img alt="1 > 0" src="./y.png"> then <b>y</b>';
  const found = [...html.matchAll(tag)].map((m) => m[0]);
  assert.deepEqual(found, ['<abbr title="a > b">', "</abbr>", '<img alt="1 > 0" src="./y.png">', "<b>", "</b>"]);
  assert.deepEqual([...'<p class="x">text</p>'.matchAll(tag)].map((m) => m[1]), ["p", "p"], "the name is the first capture");
});

/**
 * Annotating a page for an agent. Run with
 * `node --test desktop/src/views/_workbench/annotationModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import {
  EMPTY_DRAFT,
  addAnnotation,
  annotationChips,
  annotationLabel,
  annotationsContent,
  annotationsKey,
  annotationsTarget,
  marksOf,
  removeAnnotation,
  setMessage,
  staleWords } from "./annotationModel.mjs";
import { messageBody } from "./editorAgentModel.mjs";

const pick = (selector, tag = "button.cta", text = "Buy") => ({ selector, tag, excerpt: `<${tag.split(".")[0]}>${text}</${tag.split(".")[0]}>`, text });

test("an annotation is added with the next number, replaced in place when its element is pointed at again, removed with the numbers kept in step — and a note with no words adds nothing", () => {
  let d = addAnnotation(EMPTY_DRAFT, pick("#save"), " make it blue ");
  d = addAnnotation(d, pick("body > ul > li:nth-of-type(2)", "li.item", "two"), "bigger");
  d = addAnnotation(d, pick("#cancel", "button", "Cancel"), "hide it");
  assert.deepEqual(
    d.annotations.map((a) => [a.id, a.selector, a.note]),
    [
      [1, "#save", "make it blue"],
      [2, "body > ul > li:nth-of-type(2)", "bigger"],
      [3, "#cancel", "hide it"],
    ],
  );
  assert.deepEqual(marksOf(d), [
    { selector: "#save", n: 1, note: "make it blue" },
    { selector: "body > ul > li:nth-of-type(2)", n: 2, note: "bigger" },
    { selector: "#cancel", n: 3, note: "hide it" },
  ], "each badge carries its note — what the page's box opens with");
  const again = addAnnotation(d, pick("#save", "button#save", "Save"), "make it green");
  assert.equal(again.annotations.length, 3, "the same element is one annotation");
  assert.deepEqual([again.annotations[0].id, again.annotations[0].note, again.annotations[0].tag], [1, "make it green", "button#save"], "its note and its look are the newer, its number the same");
  assert.equal(again.seq, d.seq, "no number was spent");
  assert.equal(marksOf(again)[0].note, "make it green");
  const fewer = removeAnnotation(again, 2);
  assert.deepEqual(marksOf(fewer).map((m) => [m.selector, m.n]), [
    ["#save", 1],
    ["#cancel", 2],
  ], "the numbers close up");
  assert.equal(addAnnotation(fewer, pick("#x"), "   "), fewer, "no words, no annotation");
  assert.equal(setMessage(fewer, "and keep it tidy").message, "and keep it tidy");
  assert.equal(EMPTY_DRAFT.annotations.length, 0);
  assert.equal(EMPTY_DRAFT.message, "");
});

test("a row reads the number, the element as an inspector names it and the change; a lost element is said", () => {
  assert.equal(annotationLabel(1, { tag: "button.cta", note: "make it blue" }), "1 · <button.cta> make it blue");
  assert.match(staleWords([2, 3], 2), /^Not on the page any more/);
  assert.equal(staleWords([2, 3], 1), null);
});

test("the annotations become one chip each, in number order, and the request is an edit under the Git › Changes contract", () => {
  let d = addAnnotation(EMPTY_DRAFT, pick("#save"), "make it blue");
  d = addAnnotation(d, pick("#cancel", "button", "Cancel"), "hide it");
  const page = { kind: "file", path: "www/index.html" };
  const chips = annotationChips(page, d.annotations);
  assert.deepEqual(
    chips.map((c) => [c.kind, c.page.path, c.selector, c.note]),
    [
      ["annotation", "www/index.html", "#save", "make it blue"],
      ["annotation", "www/index.html", "#cancel", "hide it"],
    ],
  );
  assert.equal(chips[0].excerpt, "<button>Buy</button>");
  assert.equal(annotationsTarget(1, page), "the annotated element in index.html");
  assert.equal(annotationsTarget(3, page), "the 3 annotated elements in index.html");
  const served = { kind: "url", url: "http://localhost:5173/pricing" };
  assert.deepEqual(annotationChips(served, d.annotations)[0].page, served, "a page the browser showed: the chip carries its URL");
  assert.equal(annotationsTarget(1, served), "the annotated element of the page at http://localhost:5173/pricing");
  assert.equal(annotationsTarget(2, served), "the 2 annotated elements of the page at http://localhost:5173/pricing");
  assert.equal(annotationsContent("", 1), "The annotated element is attached as a chip with the change wanted for it.");
  assert.equal(annotationsContent("  Keep the brand colours. ", 2), "Keep the brand colours.\n\nEach of the 2 annotated elements is attached as a chip with the change wanted for it.");
  const body = messageBody({ mode: "edit", agentId: "designer", text: annotationsContent("Keep the brand colours.", 2), target: annotationsTarget(2, page), chips });
  assert.deepEqual(body.mentions, ["designer"]);
  assert.equal(body.context.length, 2);
  assert.ok(body.content.startsWith("Keep the brand colours.\n\nEach of the 2 annotated elements"), body.content);
  assert.ok(body.content.endsWith("Edit the 2 annotated elements in index.html directly in the file. Do not commit or push — I will keep or undo the change in this conversation."), body.content);
});

test("a page's draft is kept under its scope and its page — a file's under its path, a served page's under its URL", () => {
  assert.equal(annotationsKey("workstream:w1", { kind: "file", path: "www/index.html" }), "workstream:w1|annotations|file:www/index.html");
  assert.equal(annotationsKey("workstream:w1", { kind: "url", url: "http://localhost:5173/" }), "workstream:w1|annotations|url:http://localhost:5173/", "a served page keeps a draft of its own");
});

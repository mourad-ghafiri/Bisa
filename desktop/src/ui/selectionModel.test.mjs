import assert from "node:assert/strict";
import { test } from "node:test";
import { endsSelection } from "./selectionModel.mjs";

/** A tiny tree: `contains` is ancestry, as the DOM's is (a node contains itself). */
function node(name, parent = null) {
  const self = { name, parent, contains: (other) => { for (let n = other; n; n = n.parent) if (n === self) return true; return false; } };
  return self;
}
const page = node("page");
const rendering = node("rendering", page);
const paragraph = node("p", rendering);
const link = node("a", paragraph);
const sidebar = node("sidebar", page);

const selection = (collapsed, ...ancestors) => ({ isCollapsed: collapsed, rangeCount: ancestors.length, getRangeAt: (i) => ({ commonAncestorContainer: ancestors[i] }) });

test("a plain click leaves nothing selected, and opens the link", () => {
  assert.equal(endsSelection(selection(true, link), rendering), false);
  assert.equal(endsSelection(selection(false), rendering), false, "no range at all");
  assert.equal(endsSelection(null, rendering), false);
  assert.equal(endsSelection(selection(false, link), null), false);
});

test("a drag that ends on a link, or a double-click inside one, selected text of the rendering: not a click", () => {
  assert.equal(endsSelection(selection(false, link), rendering), true, "inside the anchor itself");
  assert.equal(endsSelection(selection(false, paragraph), rendering), true, "across a paragraph, ending on the link");
  assert.equal(endsSelection(selection(false, page), rendering), true, "a selection that runs through the whole rendering from outside it");
});

test("text selected somewhere else on the page does not disarm a link here", () => {
  assert.equal(endsSelection(selection(false, sidebar), rendering), false);
  assert.equal(endsSelection(selection(false, sidebar, link), rendering), true, "any of several ranges inside counts");
});

/**
 * The one door: a message with arguments and a plural, an attribute, a
 * `Text` from the node, and a miss that shows its id and is told once.
 * Run with `node --test desktop/src/i18n/l10n.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { attr, has, install, locale, onMissing, t, tx } from "./l10n.mjs";

const FTL = `
-bisa = Bisa
greeting = Welcome to { -bisa }, { $name }.
needs = { $n ->
    [0] Nothing needs you
    [one] 1 needs you
   *[other] { $n } need you
}
field = Name
    .placeholder = Your name
    .aria = The name field
`;

test("a message renders with its arguments, a plural is the message's choice, and nothing is isolated", () => {
  const errors = install("en", [FTL]);
  assert.deepEqual(errors, []);
  assert.equal(locale(), "en");
  assert.equal(t("greeting", { name: "Ada" }), "Welcome to Bisa, Ada.");
  assert.equal(t("needs", { n: 0 }), "Nothing needs you");
  assert.equal(t("needs", { n: 1 }), "1 needs you");
  assert.equal(t("needs", { n: 3 }), "3 need you");
  assert.ok(!t("greeting", { name: "Ada" }).includes("⁨"), "no FSI mark");
  assert.ok(has("needs") && !has("nothing"));
});

test("an attribute is a widget's other string, and a Text from the node is rendered here", () => {
  install("en", [FTL]);
  assert.equal(t("field"), "Name");
  assert.equal(attr("field", "placeholder"), "Your name");
  assert.equal(attr("field", "aria"), "The name field");
  assert.equal(tx({ id: "needs", args: { n: 2 } }), "2 need you");
  assert.equal(tx({ id: "field" }), "Name");
  assert.equal(tx(null), "");
});

test("a miss shows its id, is told once, and is forgotten by the next install", () => {
  install("en", [FTL]);
  const heard = [];
  const off = onMissing((id) => heard.push(id));
  assert.equal(t("nothing-here", { n: 1 }), "nothing-here");
  assert.equal(t("nothing-here"), "nothing-here");
  assert.equal(attr("field", "help"), "field.help");
  assert.deepEqual(heard, ["nothing-here", "field.help"], "once each");
  install("en", [FTL]);
  assert.equal(t("nothing-here"), "nothing-here");
  assert.deepEqual(heard, ["nothing-here", "field.help", "nothing-here"], "forgotten by the install: the same miss is told again");
  off();
});

test("a broken message is left out and the rest of the file stands", () => {
  // Fluent's parser drops what it cannot read and says nothing; `install` reports what it is told — an id defined twice.
  install("en", ["good = Good\nbad = { $n ->\n  [one] one\n", "other = Other"]);
  assert.ok(!has("bad"), "the unclosed select is not a message");
  assert.equal(t("good"), "Good");
  assert.equal(t("other"), "Other");
});

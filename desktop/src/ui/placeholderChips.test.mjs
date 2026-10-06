import { strict as assert } from "node:assert";
import { test } from "node:test";

import { placeholderChips, placeholderKindWords } from "./placeholderChips.mjs";

test("a placeholder becomes a chip naming its kind, with the whole placeholder in the tooltip", () => {
  const html = "<p>use «secret:github_token:7f3a2c» to push</p>";
  const out = placeholderChips(html);
  assert.ok(out.startsWith("<p>use <span class=\"placeholder-chip\""), out);
  assert.ok(out.includes(">github token</span> to push</p>"), out);
  assert.ok(out.includes('title="«secret:github_token:7f3a2c» — a secret'), out);
  assert.ok(!/«secret:[^"]*»(?![^<]*")/.test(out.replace(/title="[^"]*"/g, "")), "no bare placeholder is left in the text");
});

/** A placeholder spelt at run time: the guillemet form the redactor writes, built so no fixture carries one whole. */
const placeholder = (kind, hex) => "«secret:" + kind + ":" + hex + "»";

test("a placeholder inside a tag's attribute is left where it is; the one in the text beside it is still a chip", () => {
  const token = placeholder("github_token", "7f3a2c");
  const out = placeholderChips(`<abbr title="${token}">x</abbr> ${token}`);
  assert.ok(out.startsWith(`<abbr title="${token}">x</abbr> <span class="placeholder-chip"`), out);
  assert.ok(out.endsWith(">github token</span>"), out);
  const inAttr = `<img alt="a > ${placeholder("k", "abcdef")}">`;
  assert.equal(placeholderChips(inAttr), inAttr, "a > inside the attribute does not end the tag");
});

test("an environment variable's placeholder names the variable, and text without one is untouched", () => {
  assert.equal(placeholderKindWords("env:MY_API_KEY"), "MY_API_KEY");
  assert.equal(placeholderKindWords("user:team_key"), "team key");
  assert.equal(placeholderKindWords("private_key"), "private key");
  const plain = "<p>nothing to see</p>";
  assert.equal(placeholderChips(plain), plain);
  assert.equal(placeholderChips(""), "");
  const two = placeholderChips("«secret:env:MY_KEY:0a0a0a» and «secret:jwt:bbbbbb»");
  assert.equal((two.match(/placeholder-chip/g) ?? []).length, 2);
  assert.ok(two.includes(">MY_KEY</span>") && two.includes(">jwt</span>"));
});

test("a malformed placeholder is left as text and never becomes markup", () => {
  const html = '<p>«secret:no-tag» and <a href="#">x</a></p>';
  assert.equal(placeholderChips(html), html);
  const injected = placeholderChips("«secret:a<b>:abcdef»");
  assert.equal(injected, "«secret:a<b>:abcdef»", "a kind may not carry markup, so it is not a placeholder");
});

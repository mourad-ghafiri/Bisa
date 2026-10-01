/**
 * The secret field's rules. Run with `node --test desktop/src/ui/secretInputModel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import { test } from "node:test";
import { MASK, eyeLabel, foldedWords, isMasked, replaceOnFocus, secretView } from "./secretInputModel.mjs";

test("the mask is the core's word, and only that word reads as masked", () => {
  const core = readFileSync(new URL("../../../crates/bisa-core/src/mcp.rs", import.meta.url), "utf8");
  assert.ok(core.includes(`pub const MASK: &str = "${MASK}";`), "MASK mirrors bisa_core::mcp::MASK");
  assert.equal(isMasked(MASK), true);
  assert.equal(isMasked("••••••••"), false, "eight dots are a value somebody typed");
  assert.equal(isMasked(""), false);
});

test("a typed secret is hidden by default and the eye shows it — the draft, never anything else", () => {
  const hidden = secretView({ draft: "sk-live-1", what: "key" });
  assert.equal(hidden.shows, "draft");
  assert.equal(hidden.value, "sk-live-1");
  assert.equal(hidden.inputType, "password");
  assert.equal(hidden.canReveal, true);
  assert.deepEqual(hidden.eye, { label: "Show the key", pressed: false, title: "Show the key" });
  const shown = secretView({ draft: "sk-live-1", revealed: true, what: "key" });
  assert.equal(shown.inputType, "text");
  assert.equal(shown.eye.label, "Hide the key");
  assert.equal(shown.eye.pressed, true);
  assert.equal(secretView({ draft: "x", stored: true }).shows, "draft", "a draft wins over a stored one: it is what will be sent");
});

test("a stored secret with nothing typed shows the mask, dim, and the eye has nothing to show", () => {
  const v = secretView({ draft: "", stored: true, revealed: true, what: "token" });
  assert.equal(v.shows, "stored");
  assert.equal(v.value, MASK);
  assert.equal(v.inputType, "text", "the mask is drawn as it is — dots hiding dots would say nothing");
  assert.equal(v.canReveal, false);
  assert.match(v.eye.title, /never shows a token back/);
  assert.match(v.eye.title, /Type to replace/);
  assert.equal(v.dim, true);
  assert.equal(secretView({ draft: MASK, stored: true }).shows, "stored", "the node's mask in the draft is the stored state");
});

test("nothing typed and nothing stored is an empty box that says so", () => {
  const v = secretView({ draft: "", stored: false });
  assert.equal(v.shows, "empty");
  assert.equal(v.value, "");
  assert.equal(v.placeholder, "not set");
  assert.equal(v.canReveal, false);
  assert.equal(secretView({ draft: null }).shows, "empty");
});

test("focus selects the whole box exactly when typing should replace a stored value", () => {
  assert.equal(replaceOnFocus(MASK, true), true);
  assert.equal(replaceOnFocus("", true), true);
  assert.equal(replaceOnFocus("", false), false);
  assert.equal(replaceOnFocus("typed", true), false);
});

test("the eye's words, and a folded PEM block", () => {
  assert.equal(eyeLabel(false), "Show the secret");
  assert.equal(eyeLabel(true, "value"), "Hide the value");
  assert.equal(foldedWords(""), "");
  assert.equal(foldedWords("-----BEGIN-----\nabc\n-----END-----"), `${MASK} · 3 lines`);
  assert.equal(foldedWords("one\n\n"), `${MASK} · 1 line`);
});

/**
 * The words the platform types into a harness's prompt (ide/06 §Start words).
 * Run with `node --test src/terminal/typedWordsModel.test.mjs` from `desktop/`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { ENTER, ENTER_AFTER_MS, typedWords } from "./typedWordsModel.mjs";

test("a program that asked for bracketed paste gets the block wrapped, every newline a carriage return, and Enter apart", () => {
  const typed = typedWords("Edit the two elements.\n\n--- context ---\n[annotation] a · b\n", true);
  assert.equal(typed.text, "\u001b[200~Edit the two elements.\r\r--- context ---\r[annotation] a · b\u001b[201~");
  assert.equal(typed.enter, ENTER);
  assert.equal(ENTER, "\r");
  assert.ok(ENTER_AFTER_MS > 0 && ENTER_AFTER_MS < 1000, "a beat, not a wait");
});

test("a program that did not ask gets one line, the newlines folded to a space, so nothing submits early", () => {
  const typed = typedWords("Edit the element.\n\n```html\n<b>x</b>\n```\nmake it blue\n", false);
  assert.equal(typed.text, "Edit the element. ```html <b>x</b> ``` make it blue");
  assert.equal(typed.enter, "\r");
  assert.ok(!typed.text.includes("\n") && !typed.text.includes("\r"));
});

test("Windows line ends are newlines first; nothing is typed for nothing", () => {
  assert.equal(typedWords("a\r\nb", true).text, "\u001b[200~a\rb\u001b[201~");
  assert.deepEqual(typedWords("   \n ", true), { text: "", enter: "" });
  assert.deepEqual(typedWords(null, false), { text: "", enter: "" });
});

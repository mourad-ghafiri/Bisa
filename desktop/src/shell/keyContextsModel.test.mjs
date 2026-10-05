/**
 * Which scopes a key is live in, from where it landed — and that every scope
 * named here is one the keymap ranks. Run with
 * `node --test desktop/src/shell/keyContextsModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { WITHIN, scopesOf } from "./keyContextsModel.mjs";
import { WHENS, commandForEvent, resolveKeymap } from "./keymapModel.mjs";

const at = (within = {}, rest = {}) => scopesOf({ root: true, strip: true, designer: false, within, ...rest });

test("outside the IDE nothing but the document, the terminal, the browser, a tree and the designer can be live", () => {
  assert.deepEqual(scopesOf({ root: false, strip: false, designer: false, within: {} }), []);
  assert.deepEqual(scopesOf({ root: false, strip: true, designer: false, within: { monaco: true } }), ["document"], "an editor off a root is a document, never `editor`, and a strip alone is not `tabs`");
  assert.deepEqual(scopesOf({ root: false, strip: false, designer: true, within: {} }), ["designer"]);
  assert.deepEqual(scopesOf({ root: false, strip: false, designer: false, within: { files: true } }), ["files"], "the goal inspector's tree owns its keys");
  assert.deepEqual(scopesOf({ root: false, strip: false, designer: false, within: undefined }), [], "no facts about the target is no scope");
});

test("on a root: workbench always, tabs with a strip, and the target's own scope on top", () => {
  assert.deepEqual(at(), ["workbench", "tabs"]);
  assert.deepEqual(at({}, { strip: false }), ["workbench"]);
  assert.deepEqual(at({ monaco: true }), ["workbench", "tabs", "editor", "document"]);
  assert.deepEqual(at({ rendered: true }), ["workbench", "tabs", "document"], "a rendering is a document and not an editor");
  assert.deepEqual(at({ conflict: true }), ["workbench", "tabs", "document"]);
  assert.deepEqual(at({ document: true }), ["workbench", "tabs", "document"], "a document's own chrome — its bar, its mode control — is the document, never the editor");
  assert.deepEqual(scopesOf({ root: false, strip: false, designer: false, within: { document: true } }), ["document"], "on any route");
  assert.deepEqual(at({ terminal: true }), ["workbench", "tabs", "terminal"]);
  assert.deepEqual(at({ browser: true }), ["workbench", "tabs", "browser"]);
  assert.deepEqual(at({ files: true }), ["workbench", "tabs", "files"]);
});

test("the designer's scope is never live on a root: the IDE's mode chord is the workbench's there", () => {
  assert.ok(!at({}, { designer: true }).includes("designer"));
});

test("every scope that can be live is one the keymap knows, and every one the keymap knows can be live", () => {
  const all = new Set(["global"]);
  for (const root of [true, false]) {
    for (const fact of [null, ...Object.keys(WITHIN)]) {
      for (const s of scopesOf({ root, strip: true, designer: true, within: fact ? { [fact]: true } : {} })) all.add(s);
    }
  }
  assert.deepEqual([...all].sort(), [...WHENS].sort());
});

test("the narrowest live scope takes a chord two scopes hold: ⌘⇧A in the editor attaches, elsewhere it is the Inbox", () => {
  const keymap = resolveKeymap("default", {});
  const key = { key: "a", metaKey: true, ctrlKey: false, shiftKey: true, altKey: false };
  assert.equal(commandForEvent(keymap, key, at({ monaco: true }), true), "attach_selection");
  assert.equal(commandForEvent(keymap, key, at({ terminal: true }), true), "inbox");
  assert.equal(commandForEvent(keymap, key, scopesOf({ root: false, strip: false, designer: false, within: { monaco: true } }), true), "inbox", "an editor off a root attaches nothing");
});

test("the hook reads the DOM with the model's selectors and decides nothing itself", () => {
  const hook = readFileSync(new URL("./keyContexts.ts", import.meta.url), "utf8");
  assert.ok(hook.includes("scopesOf({"), "the scopes are the model's");
  assert.ok(!/out\.push\(/.test(hook), "no scope is pushed by hand");
  for (const selector of Object.values(WITHIN)) assert.ok(!hook.includes(`"${selector}"`), `${selector} is spelt once, in the model`);
});
